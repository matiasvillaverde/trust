use trust_model::{Account, Database};

pub struct Trust {
    database: Box<dyn Database>,
}

impl Trust {
    pub fn new(database: Box<dyn Database>) -> Self {
        Trust { database: database }
    }

    pub fn create_account(
        &mut self,
        name: &str,
        description: &str,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        let account = self.database.create_account(name, description);
        account
    }
}

// TODO: Add tests
