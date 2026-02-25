//! Central application state for the TUI.

use std::sync::mpsc::{self, Receiver, Sender};

use crate::models::{Instance, TunnelProcess, VpnConfig};

// ── Tab ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab { Instances, Tunnels, Tools, Vpn }

const TAB_COUNT: usize = 4;

impl Tab {
    pub fn titles() -> &'static [&'static str] {
        &["Instances", "Tunnels", "Tools", "VPN"]
    }
    pub fn index(self) -> usize {
        match self { Self::Instances => 0, Self::Tunnels => 1, Self::Tools => 2, Self::Vpn => 3 }
    }
    pub fn from_index(i: usize) -> Self {
        match i { 1 => Self::Tunnels, 2 => Self::Tools, 3 => Self::Vpn, _ => Self::Instances }
    }
    pub fn next(self) -> Self { Self::from_index((self.index() + 1) % TAB_COUNT) }
    pub fn prev(self) -> Self { Self::from_index((self.index() + TAB_COUNT - 1) % TAB_COUNT) }
}

// ── Popup / modal ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Popup {
    None,
    Help,
    /// Single-line text input. (title, placeholder, current_input, callback_tag)
    Input { title: String, placeholder: String, value: String, tag: InputTag },
    /// Scrollable list selection.
    Select { title: String, items: Vec<String>, selected: usize, tag: InputTag },
    /// Confirm dialog.
    Confirm { message: String, tag: ConfirmTag, selected_yes: bool },
    /// Show result text (success or error)
    Result { title: String, body: String, is_error: bool },
    /// Spinner overlay
    Loading { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputTag {
    NewTunnelPattern,
    NewTunnelLocalPort,
    NewTunnelRemotePort,
    NewTunnelUrl,
    NewTunnelUrlLocalPort,
    NewTunnelUrlRemotePort,
    NewTunnelBastionPattern,
    NewTunnelBastionHost,
    NewTunnelBastionLocalPort,
    NewTunnelBastionRemotePort,
    LoginProfile,
    ResolveUrl,
    TestPort,
    SwitchProfile,
    SwitchRegion,
    VpnMfaCode,
    VpnSetupUsername,
    VpnSetupPassword,
    VpnSetupOvpnPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmTag {
    StopTunnel(usize),
    StopAllTunnels,
    StopInstance,
    ForceStopInstance,
}

// ── Background task messages ──────────────────────────────────────────────────

#[derive(Debug)]
pub enum BgMessage {
    InstancesLoaded(crate::error::Result<Vec<Instance>>),
    TunnelsLoaded(Vec<TunnelProcess>),
    TunnelStarted(crate::error::Result<TunnelProcess>),
    ActionDone(crate::error::Result<String>),
    VpnConnected(crate::error::Result<String>),
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    pub profile: String,
    pub region: String,
    pub tab: Tab,
    pub tunnel_refresh_ticks: u32,

    // Instances tab
    pub instances: Vec<Instance>,
    pub instance_selected: usize,
    pub instance_filter: String,
    pub instance_filter_active: bool,

    // Tunnels tab
    pub tunnels: Vec<TunnelProcess>,
    pub tunnel_selected: usize,

    // Tools tab
    pub tool_selected: usize,

    // VPN tab
    pub vpn_selected: usize,
    pub vpn_config: VpnConfig,
    pub vpn_status: String,

    // Popup / modal
    pub popup: Popup,

    // Loading
    pub loading: bool,
    pub loading_message: String,

    // Spinner
    pub spinner_tick: u8,

    // Background channel
    pub tx: Sender<BgMessage>,
    pub rx: Receiver<BgMessage>,

    // Wizard state (multi-step input buffer)
    pub wizard_buf: WizardBuf,

    pub quit: bool,
    pub status_msg: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct WizardBuf {
    pub pattern: String,
    pub local_port: String,
    pub remote_port: String,
    pub url: String,
    pub bastion: String,
    pub host: String,
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            profile: crate::aws::get_profile(),
            region: crate::aws::get_region(None),
            tab: Tab::Instances,
            tunnel_refresh_ticks: 0,
            instances: vec![],
            instance_selected: 0,
            instance_filter: String::new(),
            instance_filter_active: false,
            tunnels: vec![],
            tunnel_selected: 0,
            tool_selected: 0,
            vpn_selected: 0,
            vpn_config: crate::vpn::load_config().unwrap_or_default(),
            vpn_status: if crate::vpn::is_connected() {
                format!("CONNECTED ({})", crate::vpn::get_vpn_ip().unwrap_or_else(|| "?".into()))
            } else {
                "DISCONNECTED".into()
            },
            popup: Popup::None,
            loading: false,
            loading_message: String::new(),
            spinner_tick: 0,
            tx,
            rx,
            wizard_buf: WizardBuf::default(),
            quit: false,
            status_msg: None,
        }
    }

    pub fn refresh_instances(&mut self) {
        self.loading = true;
        self.loading_message = "Loading instances...".to_string();
        let tx = self.tx.clone();
        let profile = std::env::var("AWS_PROFILE").ok().filter(|s| !s.is_empty());
        std::thread::spawn(move || {
            let _ = tx.send(BgMessage::InstancesLoaded(
                crate::aws::list_instances(profile.as_deref()),
            ));
        });
    }

    pub fn refresh_tunnels(&mut self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(BgMessage::TunnelsLoaded(crate::tunnel::detect_tunnels()));
        });
    }

    pub fn poll_bg(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.loading = false;
            match msg {
                BgMessage::InstancesLoaded(Ok(instances)) => {
                    self.instances = instances;
                    self.instance_selected = self.instance_selected
                        .min(self.instances.len().saturating_sub(1));
                }
                BgMessage::InstancesLoaded(Err(e)) => {
                    self.popup = Popup::Result { title: "Error".into(), body: e.to_string(), is_error: true };
                }
                BgMessage::TunnelsLoaded(tunnels) => {
                    self.tunnels = tunnels;
                    self.tunnel_selected = self.tunnel_selected
                        .min(self.tunnels.len().saturating_sub(1));
                }
                BgMessage::TunnelStarted(Ok(tp)) => {
                    let latency_str = tp.latency_ms
                        .map(|ms| format!(" ({}ms)", ms))
                        .unwrap_or_default();
                    let body = format!(
                        "localhost:{} -> {}:{}{}",
                        tp.local_port,
                        tp.remote_host.as_deref().unwrap_or(&tp.instance_name),
                        tp.remote_port,
                        latency_str,
                    );
                    self.tunnels.push(tp);
                    self.popup = Popup::Result { title: "Tunnel Started".into(), body, is_error: false };
                }
                BgMessage::TunnelStarted(Err(e)) => {
                    self.popup = Popup::Result { title: "Tunnel Error".into(), body: e.to_string(), is_error: true };
                }
                BgMessage::ActionDone(Ok(msg)) => {
                    self.popup = Popup::Result { title: "Done".into(), body: msg, is_error: false };
                    self.refresh_instances();
                }
                BgMessage::ActionDone(Err(e)) => {
                    self.popup = Popup::Result { title: "Error".into(), body: e.to_string(), is_error: true };
                }
                BgMessage::VpnConnected(Ok(msg)) => {
                    self.vpn_status = if crate::vpn::is_connected() {
                        format!("CONNECTED ({})", crate::vpn::get_vpn_ip().unwrap_or_else(|| "?".into()))
                    } else {
                        "DISCONNECTED".into()
                    };
                    self.popup = Popup::Result { title: "VPN".into(), body: msg, is_error: false };
                }
                BgMessage::VpnConnected(Err(e)) => {
                    self.vpn_status = "DISCONNECTED".into();
                    self.popup = Popup::Result { title: "VPN Error".into(), body: e.to_string(), is_error: true };
                }
            }
        }
    }

    pub fn filtered_instances(&self) -> Vec<&Instance> {
        let filter = self.instance_filter.to_lowercase();
        self.instances.iter().filter(|i| {
            filter.is_empty()
                || i.name.to_lowercase().contains(&filter)
                || i.id.to_lowercase().contains(&filter)
                || i.instance_type.to_lowercase().contains(&filter)
        }).collect()
    }

    pub fn selected_instance(&self) -> Option<&Instance> {
        self.filtered_instances().get(self.instance_selected).copied()
    }

    pub fn selected_tunnel(&self) -> Option<&TunnelProcess> {
        self.tunnels.get(self.tunnel_selected)
    }

    pub fn tick_spinner(&mut self) {
        self.spinner_tick = self.spinner_tick.wrapping_add(1);
        // Auto-refresh tunnels every ~15 s (200 ms tick × 75 = 15 s)
        self.tunnel_refresh_ticks = self.tunnel_refresh_ticks.wrapping_add(1);
        if self.tunnel_refresh_ticks >= 75 {
            self.tunnel_refresh_ticks = 0;
            self.refresh_tunnels();
        }
    }
}
