pub mod evdev_hook;
pub mod uinput_sim;

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    DirectModePressed,
    DirectModeReleased,
    TranslateModePressed,
    TranslateModeReleased,
    MouseLeftClicked,
}
