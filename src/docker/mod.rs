mod containers;
mod stats;
mod images;
mod files;
mod network;
mod edit;
mod quota;

pub use containers::*;
pub use stats::*;
pub use images::*;
pub use files::*;
pub use network::*;
pub use edit::*;
pub use quota::{apply_btrfs_quota, apply_ext4_quota, apply_xfs_quota, apply_zfs_quota, btrfs_mount_for, ext4_pquota_mount, parse_disk_limit, remove_btrfs_quota, remove_ext4_quota, remove_xfs_quota, remove_zfs_quota, xfs_pquota_mount, zfs_dataset_for};

use bollard::Docker;
use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub status: String,
    pub state: String,
    pub cpu_usage: String,
    pub ram_usage: String,
    /// Internal SQLite id. 0 if not yet resolved from DB.
    pub db_id: i64,
    /// Owner username. Empty string if not yet resolved from DB.
    pub owner: String,
}

pub async fn get_docker_client() -> Result<Docker> {
    Docker::connect_with_socket_defaults().context("Failed to connect to Docker socket")
}
