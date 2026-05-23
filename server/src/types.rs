use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyName {
    Space,
    Left,
    Right,
    Up,
    Down,
    Enter,
    Escape,
    F,
    M,
    C,
    J,
    K,
    L,
    N,
    Comma,
    Period,
    Tab,
    Backspace,
    Delete,
    Home,
    End,
    PageUp,
    PageDown,
    A,
    D,
    R,
    T,
    V,
    W,
    X,
    Z,
    F12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Modifier {
    Shift,
    Ctrl,
    Alt,
    Win,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseAction {
    Press,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionName {
    PlayPause,
    SeekBack10,
    SeekFwd10,
    SeekBack30,
    SeekFwd30,
    VolUp,
    VolDown,
    Mute,
    Fullscreen,
    Captions,
    NextEpisode,
    SpeedDown,
    SpeedUp,
}

pub type ProfileBindings = HashMap<ActionName, String>;
pub type ActionBindings = HashMap<ProfileName, ProfileBindings>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileName {
    Auto,
    Generic,
    Youtube,
    Netflix,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Command {
    Key {
        key: KeyName,
        #[serde(default)]
        mods: Vec<Modifier>,
    },
    Combo {
        keys: Vec<KeyName>,
    },
    Action {
        name: ActionName,
        profile: Option<ProfileName>,
    },
    MouseMove {
        dx: f32,
        dy: f32,
    },
    MouseClick {
        button: MouseButton,
    },
    MouseButton {
        button: MouseButton,
        action: MouseAction,
    },
    MouseScroll {
        dx: f32,
        dy: f32,
    },
    TypeText {
        text: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMessage<'a> {
    Hello {
        version: &'a str,
        profiles: &'a [ProfileName],
        bindings: &'a ActionBindings,
    },
    State {
        active: bool,
    },
    Ack {
        #[serde(skip_serializing_if = "Option::is_none")]
        suppressed: Option<bool>,
    },
    Error {
        message: String,
    },
}

pub const ALL_PROFILES: &[ProfileName] = &[
    ProfileName::Auto,
    ProfileName::Generic,
    ProfileName::Youtube,
    ProfileName::Netflix,
];

pub const ALL_ACTIONS: &[ActionName] = &[
    ActionName::PlayPause,
    ActionName::SeekBack10,
    ActionName::SeekFwd10,
    ActionName::SeekBack30,
    ActionName::SeekFwd30,
    ActionName::VolUp,
    ActionName::VolDown,
    ActionName::Mute,
    ActionName::Fullscreen,
    ActionName::Captions,
    ActionName::NextEpisode,
    ActionName::SpeedDown,
    ActionName::SpeedUp,
];

pub const VERSION: &str = "0.9.0";
