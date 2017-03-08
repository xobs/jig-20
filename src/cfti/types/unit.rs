/// Generic Unit implementations

#[derive(Clone)]
pub struct SimpleUnit {
    id: String,
    kind: String,
    name: String,
    description: String,
}

pub trait Unit {
    fn id(&self) -> &str;
    fn kind(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;

    fn as_simple_unit(&self) -> SimpleUnit {
        SimpleUnit {
            id: self.id().to_string(),
            kind: self.kind().to_string(),
            name: self.name().to_string(),
            description: self.description().to_string(),
        }
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
}