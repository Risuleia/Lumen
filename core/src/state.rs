#[derive(Debug, Clone)]
pub enum IslandState {
    IdleDormant,
    BriefPulse(PulseReason),
    ActiveWidget(ActivityKind),
    PrivacyIndicator(PrivacyKind),
    ControlCenter
}

#[derive(Debug, Clone)]
pub enum PulseReason {
    Bluetooth,
    Wifi,
    Battery,
    Usb,
    Other(String)
}

#[derive(Debug, Clone)]
pub enum ActivityKind {
    Media,
    Call,
    Timer
}

#[derive(Debug, Clone)]
pub enum PrivacyKind {
    Microphone,
    Camera,
    ScreenCapture
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveMode {
    Camera,
    Mic,
    Media,
    Idle
}