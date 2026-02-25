//! Domain models and raw AWS CLI JSON deserialization types.

#![allow(dead_code)]

use serde::Deserialize;

// ── Domain models ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum InstanceState {
    Running,
    Stopped,
    Pending,
    Stopping,
    Other(String),
}

impl InstanceState {
    pub fn from_str(s: &str) -> Self {
        match s {
            "running"  => Self::Running,
            "stopped"  => Self::Stopped,
            "pending"  => Self::Pending,
            "stopping" => Self::Stopping,
            other      => Self::Other(other.to_string()),
        }
    }
    pub fn as_str(&self) -> &str {
        match self {
            Self::Running   => "running",
            Self::Stopped   => "stopped",
            Self::Pending   => "pending",
            Self::Stopping  => "stopping",
            Self::Other(s)  => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SsmStatus { Online, Offline, Unknown }

impl SsmStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Online  => "Online",
            Self::Offline => "Offline",
            Self::Unknown => "-",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TunnelStatus { Active, Down }

#[derive(Debug, Clone)]
pub struct TunnelInfo {
    pub local_port: u16,
    pub remote_port: u16,
    pub remote_host: Option<String>,
    pub status: TunnelStatus,
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub instance_type: String,
    pub state: InstanceState,
    pub private_ip: Option<String>,
    pub public_ip: Option<String>,
    pub ssm_status: SsmStatus,
    pub tunnel: Option<TunnelInfo>,
    pub security_groups: Vec<String>,
    pub security_group_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BastionInfo {
    pub id: String,
    pub name: String,
    pub ssm_online: bool,
}

#[derive(Debug, Clone)]
pub struct TunnelProcess {
    pub pid: u32,
    pub local_port: u16,
    pub remote_port: u16,
    pub remote_host: Option<String>,
    pub instance_id: String,
    pub instance_name: String,
    /// Cached connectivity result — set at detection/creation time, never in render.
    pub port_open: bool,
    /// Round-trip latency in ms for the first successful TCP connect (None if unknown).
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum TunnelTarget {
    Ec2 { instance_id: String, name: String },
    RemoteViaBastion {
        bastion_id: String,
        bastion_name: String,
        target_host: String,
        target_port: u16,
    },
}

// ── Raw JSON deserialization structs (aws cli output) ─────────────────────────

#[derive(Debug, Deserialize)]
pub struct RawInstance {
    #[serde(rename = "InstanceId")]
    pub instance_id: String,
    #[serde(rename = "InstanceType")]
    pub instance_type: String,
    #[serde(rename = "State")]
    pub state: RawInstanceState,
    #[serde(rename = "PrivateIpAddress")]
    pub private_ip: Option<String>,
    #[serde(rename = "PublicIpAddress")]
    pub public_ip: Option<String>,
    #[serde(rename = "Tags")]
    pub tags: Option<Vec<Tag>>,
    #[serde(rename = "SecurityGroups")]
    pub security_groups: Option<Vec<SecurityGroup>>,
}

#[derive(Debug, Deserialize)]
pub struct RawInstanceState {
    #[serde(rename = "Name")]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Tag {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct SecurityGroup {
    #[serde(rename = "GroupId")]
    pub group_id: String,
    #[serde(rename = "GroupName")]
    pub group_name: String,
}

#[derive(Debug, Deserialize)]
pub struct SsmInstanceInfo {
    #[serde(rename = "InstanceId")]
    pub instance_id: String,
    #[serde(rename = "PingStatus")]
    pub ping_status: String,
}

#[derive(Debug, Deserialize)]
pub struct SsmDescribeResponse {
    #[serde(rename = "InstanceInformationList")]
    pub instance_information_list: Vec<SsmInstanceInfo>,
}
