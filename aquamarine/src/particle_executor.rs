/*
 * Copyright 2020 Fluence Labs Limited
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::borrow::Cow;
use std::time::Duration;
use std::{task::Waker, time::Instant};

use avm_server::{CallResults, ParticleParameters};
use fluence_libp2p::PeerId;
use futures::{future::BoxFuture, FutureExt};
use humantime::format_duration as pretty;
use particle_protocol::Particle;

use crate::aqua_runtime::AquaRuntime;
use crate::particle_effects::ParticleEffects;
use crate::InterpretationStats;

pub(super) type Fut<RT> = BoxFuture<'static, FutResult<RT, ParticleEffects, InterpretationStats>>;

pub trait ParticleExecutor {
    type Future;
    type Particle;
    fn execute(self, p: Self::Particle, waker: Waker, current_peer_id: PeerId) -> Self::Future;
}

/// Result of a particle execution along a VM that has just executed the particle
pub struct FutResult<RT, Eff, Stats> {
    /// AVM that just executed a particle
    pub vm: Option<RT>,
    /// Effects produced by particle execution
    pub effects: Eff,
    /// Performance stats
    pub stats: Stats,
}

impl<RT: AquaRuntime> ParticleExecutor for RT {
    type Future = Fut<Self>;
    type Particle = (Particle, CallResults);

    fn execute(mut self, p: Self::Particle, waker: Waker, current_peer_id: PeerId) -> Self::Future {
        let (particle, calls) = p;
        let cloned_particle = particle.clone();
        let task = tokio::task::Builder::new().name(&format!("Particle {}", particle.id)).spawn_blocking(move || {
            let now = Instant::now();
            log::info!("Executing particle {}", particle.id);

            let particle_params = ParticleParameters {
                current_peer_id: Cow::Owned(current_peer_id.to_string()),
                init_peer_id: Cow::Owned(particle.init_peer_id.to_string()),
                particle_id: Cow::Borrowed(&particle.id),
                timestamp: particle.timestamp,
                ttl: particle.ttl,
            };
            let result = self.call(particle.script.clone(), particle.data.clone(), particle_params.clone(), calls);

            let interpretation_time = now.elapsed();
            let new_data_len = result.as_ref().map(|e| e.data.len()).ok();
            let stats = InterpretationStats { interpretation_time, new_data_len, success: result.is_ok() };

            if let Err(err) = &result {
                log::warn!("Error executing particle {:#?}: {}", particle, err)
            } else {
                let len = new_data_len.map(|l| l as i32).unwrap_or(-1);
                log::trace!(target: "execution", "Particle {} interpreted in {} [{} bytes => {} bytes]", particle.id, pretty(interpretation_time), particle.data.len(), len);
            }
            let effects = Self::into_effects(result, particle);

            waker.wake();

            FutResult {
                vm: Some(self),
                effects,
                stats,
            }
        }).expect("Could not spawn 'Particle' task");

        async move {
            let result = task.await;
            match result {
                Ok(res) => res,
                Err(err) => {
                    if err.is_cancelled() {
                        log::warn!("Particle task {} was cancelled", cloned_particle.id);
                    } else {
                        log::error!("Particle task {} panic", cloned_particle.id);
                    }
                    let stats = InterpretationStats {
                        interpretation_time: Duration::ZERO,
                        new_data_len: None,
                        success: false,
                    };
                    let effects = ParticleEffects::empty(cloned_particle);
                    FutResult {
                        vm: None,
                        effects,
                        stats,
                    }
                }
            }
        }
        .boxed()
    }
}
