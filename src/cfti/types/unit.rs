/// Generic Unit implementations

use cfti::controller::{Controller, BroadcastMessageContents, ControlMessageContents};

#[derive(Clone)]
pub struct SimpleUnit {
    id: String,
    kind: String,
    name: String,
    description: String,
    controller: Controller,
}

pub trait Unit {
    fn id(&self) -> &str;
    fn kind(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn controller(&self) -> &Controller;

    fn to_simple_unit(&self) -> SimpleUnit {
        SimpleUnit {
            id: self.id().to_string(),
            kind: self.kind().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
            controller: self.controller().clone(),
        }
    }

    fn debug(&self, msg: String) {
        Controller::debug_unit(self, msg);
    }

    fn warn(&self, msg: String) {
        Controller::warn_unit(self, msg);
    }

    fn broadcast(&self, msg: BroadcastMessageContents) {
        Controller::broadcast_unit(self, &msg);
    }

    fn control(&self, msg: ControlMessageContents) {
        Controller::control_unit(self, &msg);
    }

    fn log(&self, msg: String) {
        self.broadcast(BroadcastMessageContents::Log(msg));
    }
}

impl Unit for SimpleUnit {
    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn kind(&self) -> &str {
        self.kind.as_str()
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn description(&self) -> &str {
        self.description.as_str()
    }

    fn controller(&self) -> &Controller {
        &self.controller
    }
}