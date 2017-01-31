extern crate ini;
use self::ini::Ini;

#[derive(Debug)]
pub enum JigError {
    FileLoadError,
    MissingJigSection,
}

#[derive(Debug)]
pub struct Jig {

    /// Id: File name on disk, what other units refer to this one as.
    id: String,

    /// Name: Defines the short name for this jig.
    name: String,

    /// Description: Defines a detailed description of this jig.  May be up to one paragraph.
    description: String,

    /// Cartesian: Optional path to a program to determine if this is the jig we're running on.
    cartesian: Option<String>,

    /// DefaultCcenario: Name of the scenario to run by default.
    default_scenario: Option<String>,
}

impl Jig {
    pub fn new(id: &str, path: &str) -> Result<Jig, JigError> {

        // Load the .ini file
        let ini_file = match Ini::load_from_file(&path) {
            Err(_) => return Err(JigError::FileLoadError),
            Ok(s) => s,
        };

        let jig_section = match ini_file.section(Some("Jig")) {
            None => return Err(JigError::MissingJigSection),
            Some(s) => s,
        };

        let description = match jig_section.get("Description") {
            None => "".to_string(),
            Some(s) => s.to_string(),
        };

        let name = match jig_section.get("Name") {
            None => id.to_string(),
            Some(s) => s.to_string(),
        };

        let cartesian = match jig_section.get("Cartesian") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        let default_scenario = match jig_section.get("DefaultScenario") {
            None => None,
            Some(s) => Some(s.to_string()),
        };

        Ok(Jig {
            id: id.to_string(),
            name: name,
            description: description,

            cartesian: cartesian,
            default_scenario: default_scenario,
        })
    }

    pub fn name(&self) -> &String {
        return &self.name;
    }

    pub fn id(&self) -> &String {
        return &self.id;
    }
}