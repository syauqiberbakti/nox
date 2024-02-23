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

use fs_utils::{create_dirs, set_write_only, to_abs_path};

use bytesize::ByteSize;
use cid_utils::Hash;
use libp2p::PeerId;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ServicesConfig {
    /// Peer id of the current node
    pub local_peer_id: PeerId,
    /// Path of the blueprint directory containing blueprints and wasm modules
    pub blueprint_dir: PathBuf,
    /// Opaque environment variables to be passed on each service creation
    /// TODO: isolate envs of different modules (i.e., module A shouldn't access envs of module B)
    pub envs: HashMap<String, String>,
    /// Working dir for services
    pub workdir: PathBuf,
    /// Dir to store .wasm modules and their configs
    pub modules_dir: PathBuf,
    /// Dir to persist info about running services
    pub services_dir: PathBuf,
    /// Dir to store directories shared between services
    /// in the span of a single particle execution  
    pub particles_vault_dir: PathBuf,
    /// key that could manage services
    pub management_peer_id: PeerId,
    /// key to manage builtins services initialization
    pub builtins_management_peer_id: PeerId,
    /// Default heap size in bytes available for the module unless otherwise specified.
    pub default_service_memory_limit: Option<ByteSize>,
    /// List of allowed effector modules by CID
    pub allowed_effectors: HashMap<Hash, HashMap<String, PathBuf>>,
}

impl ServicesConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        local_peer_id: PeerId,
        base_dir: PathBuf,
        particles_vault_dir: PathBuf,
        envs: HashMap<String, String>,
        management_peer_id: PeerId,
        builtins_management_peer_id: PeerId,
        default_service_memory_limit: Option<ByteSize>,
        allowed_effectors: HashMap<Hash, HashMap<String, String>>,
    ) -> Result<Self, std::io::Error> {
        let base_dir = to_abs_path(base_dir);

        let allowed_effectors = allowed_effectors
            .into_iter()
            .map(|(cid, effector)| {
                let effector = effector
                    .into_iter()
                    .map(|(name, path_str)| {
                        let path = Path::new(&path_str);
                        match path.try_exists() {
                            Err(err) => log::warn!("cannot check effector `{path_str}`: {err}"),
                            Ok(false) => log::warn!("effector `{path_str}` does not exist"),
                            _ => {}
                        };
                        (name, path.to_path_buf())
                    })
                    .collect::<_>();
                (cid, effector)
            })
            .collect::<_>();

        let this = Self {
            local_peer_id,
            blueprint_dir: config_utils::blueprint_dir(&base_dir),
            workdir: config_utils::workdir(&base_dir),
            modules_dir: config_utils::modules_dir(&base_dir),
            services_dir: config_utils::services_dir(&base_dir),
            particles_vault_dir,
            envs,
            management_peer_id,
            builtins_management_peer_id,
            default_service_memory_limit,
            allowed_effectors,
        };

        create_dirs(&[
            &this.blueprint_dir,
            &this.workdir,
            &this.modules_dir,
            &this.services_dir,
            &this.particles_vault_dir,
        ])?;

        set_write_only(&this.particles_vault_dir)?;

        Ok(this)
    }
}
