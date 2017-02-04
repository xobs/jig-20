pub enum InterfaceError {

}

#[derive(Debug, Clone)]
pub enum InterfaceItem {
    HelloOut(String),
    HelloIn(String),
}

pub struct Interface {
}

impl Interface {
    pub fn new() -> Result<Interface, InterfaceError> {
        // Create a new channel pair.  Log messages will be broadcast to "rx"
        let mut interface = Interface {
        };

        Ok(interface)
    }
}