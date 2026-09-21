use evdev::{raw_stream::RawDevice, uinput::VirtualDevice, EventType, InputEvent, UinputAbsSetup};
use std::{ffi::CString, io};

use crate::devices::{GAME_TARGET_NAME, GAME_TARGET_PHYS, PORTAL_TARGET_NAME, PORTAL_TARGET_PHYS};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputOwner {
    Game,
    Portal,
}

pub trait TargetRouter: Send {
    fn set_owner(&mut self, owner: InputOwner);
    fn owner(&self) -> InputOwner;
    fn route(&mut self, event: InputEvent) -> io::Result<()>;
}

pub struct DisabledTargetRouter {
    owner: InputOwner,
}

impl Default for DisabledTargetRouter {
    fn default() -> Self {
        Self {
            owner: InputOwner::Portal,
        }
    }
}

impl TargetRouter for DisabledTargetRouter {
    fn set_owner(&mut self, owner: InputOwner) {
        self.owner = owner;
    }

    fn owner(&self) -> InputOwner {
        self.owner
    }

    fn route(&mut self, _event: InputEvent) -> io::Result<()> {
        Ok(())
    }
}

pub struct UinputTargetRouter {
    game: VirtualDevice,
    portal: VirtualDevice,
    owner: InputOwner,
}

impl UinputTargetRouter {
    pub fn from_source(source: &RawDevice) -> io::Result<Self> {
        Ok(Self {
            game: create_target(source, GAME_TARGET_NAME, GAME_TARGET_PHYS)?,
            portal: create_target(source, PORTAL_TARGET_NAME, PORTAL_TARGET_PHYS)?,
            owner: InputOwner::Portal,
        })
    }
}

impl TargetRouter for UinputTargetRouter {
    fn set_owner(&mut self, owner: InputOwner) {
        self.owner = owner;
    }

    fn owner(&self) -> InputOwner {
        self.owner
    }

    fn route(&mut self, event: InputEvent) -> io::Result<()> {
        // VirtualDevice::emit appends SYN_REPORT. Forward source facts as they
        // arrive and omit source synchronization records so ownership changes
        // never require an input queue or synthetic release events.
        if event.event_type() == EventType::SYNCHRONIZATION {
            return Ok(());
        }
        match self.owner {
            InputOwner::Game => self.game.emit(&[event]),
            InputOwner::Portal => self.portal.emit(&[event]),
        }
    }
}

fn create_target(source: &RawDevice, name: &str, physical_path: &str) -> io::Result<VirtualDevice> {
    let physical_path = CString::new(physical_path).expect("fixed virtual target physical path");
    let mut builder = VirtualDevice::builder()?
        .name(name)
        .input_id(source.input_id())
        .with_phys(&physical_path)?;
    if let Some(keys) = source.supported_keys() {
        builder = builder.with_keys(keys)?;
    }
    for (axis, info) in source.get_absinfo()? {
        builder = builder.with_absolute_axis(&UinputAbsSetup::new(axis, info))?;
    }
    builder.build()
}

#[cfg(test)]
pub struct RecordingTargetRouter {
    pub owner: InputOwner,
    pub game: Vec<InputEvent>,
    pub portal: Vec<InputEvent>,
}

#[cfg(test)]
impl Default for RecordingTargetRouter {
    fn default() -> Self {
        Self {
            owner: InputOwner::Portal,
            game: Vec::new(),
            portal: Vec::new(),
        }
    }
}

#[cfg(test)]
impl TargetRouter for RecordingTargetRouter {
    fn set_owner(&mut self, owner: InputOwner) {
        self.owner = owner;
    }

    fn owner(&self) -> InputOwner {
        self.owner
    }

    fn route(&mut self, event: InputEvent) -> io::Result<()> {
        match self.owner {
            InputOwner::Game => self.game.push(event),
            InputOwner::Portal => self.portal.push(event),
        }
        Ok(())
    }
}
