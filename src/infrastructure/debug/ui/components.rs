use bevy::prelude::*;

#[derive(Component)]
pub(crate) struct DebugExpandButton;

#[derive(Component)]
pub(crate) struct DebugFpsText;

#[derive(Component)]
pub(crate) struct DebugEntityCountText;

#[derive(Component)]
pub(crate) struct DebugDayCycleText;

#[derive(Component)]
pub(crate) struct DebugTweakPanel;

#[derive(Component)]
pub(crate) struct TweakPanelBuiltVersion(pub(crate) u64);

#[derive(Component)]
pub(crate) struct FolderHeader(pub(crate) String);

#[derive(Component)]
pub(crate) struct TweakSlider {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct TweakSliderFill {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct TweakSliderKnob {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct TweakSliderValueText {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct TweakToggle {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct TweakToggleText {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct TweakReadOnlyText {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct SaveConfigButton;

#[derive(Component)]
pub(crate) struct SaveConfigButtonText;

#[derive(Component)]
pub(crate) struct ColorPreview {
    pub(crate) folder: String,
    pub(crate) prefix: String,
}

#[derive(Component)]
pub(crate) struct TweakCycleEnum {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct TweakCycleText {
    pub(crate) folder: String,
    pub(crate) label: String,
}

#[derive(Component)]
pub(crate) struct TweakButton {
    pub(crate) folder: String,
    pub(crate) label: String,
}
