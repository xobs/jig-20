/// Generic Unit implementations

pub trait Unit {
    fn id(&self) -> &str;
    fn kind(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
}