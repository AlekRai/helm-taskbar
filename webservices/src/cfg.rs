use serde::{Deserialize, Serialize};

// A web service shown in its own window (mail, chat). Services sharing a `profile` share logins.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct ServiceCfg {
    pub id: String,
    pub name: String,
    pub url: String,
    // "personal" or "work": which bar group the service belongs to.
    pub group: String,
    pub profile: String,
    // Built-in adapter that counts unread items: gmail, outlook, messenger, teams or generic.
    pub adapter: String,
    pub enabled: bool,
    // Flash the bar button when this service gets new mail.
    pub flash: bool,
}

impl Default for ServiceCfg {
    fn default() -> Self {
        ServiceCfg {
            id: String::new(),
            name: String::new(),
            url: String::new(),
            group: "personal".into(),
            profile: "main".into(),
            adapter: "generic".into(),
            enabled: true,
            flash: true,
        }
    }
}

pub fn default_services() -> Vec<ServiceCfg> {
    let svc = |id: &str, name: &str, url: &str, group: &str, profile: &str, adapter: &str| ServiceCfg {
        id: id.into(),
        name: name.into(),
        url: url.into(),
        group: group.into(),
        profile: profile.into(),
        adapter: adapter.into(),
        enabled: true,
        flash: true,
    };
    vec![
        svc("gmail1", "Gmail 1", "https://mail.google.com/mail/u/0/", "personal", "main", "gmail"),
        svc("gmail2", "Gmail 2", "https://mail.google.com/mail/u/1/", "personal", "main", "gmail"),
        svc("gmail3", "Gmail 3", "https://mail.google.com/mail/u/2/", "personal", "main", "gmail"),
        svc("messenger", "Messenger", "https://www.messenger.com/", "personal", "main", "messenger"),
        svc("outlook", "Outlook", "https://outlook.office.com/mail/", "work", "main", "outlook"),
        svc("teams", "Teams", "https://teams.cloud.microsoft/", "work", "main", "teams"),
    ]
}
