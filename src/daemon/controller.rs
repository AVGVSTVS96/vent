use crate::fan::{Fan, Fans};
use crate::hold::Hold;
use crate::protocol::Request;
use crate::smc::{self, Smc};

pub struct Controller {
    smc: Smc,
    hold: Hold,
}

impl Controller {
    pub fn new(smc: Smc, hold: Hold) -> Self {
        Self { smc, hold }
    }

    fn fans(&self) -> Fans<'_> {
        Fans::new(&self.smc)
    }

    /// Runs every held fan at its held speed.
    pub fn apply(&self) -> smc::Result<()> {
        let fans = self.fans();
        for fan in fans.all()? {
            if let Some(speed) = self.hold.get(&fan.name) {
                fans.set(&fan, speed)?;
                eprintln!("vent daemon: {} {speed}", fan.name);
            }
        }
        Ok(())
    }

    /// Gives every fan back to the system, keeping the hold for later.
    pub fn restore(&self) -> smc::Result<()> {
        self.fans().release_all()?;
        eprintln!("vent daemon: every fan auto");
        Ok(())
    }

    pub fn handle(&mut self, request: Request) -> Result<String, String> {
        match request {
            Request::Get => {}
            Request::Set { fan, speed } => {
                let fans = Fans::new(&self.smc);
                let targets: Vec<Fan> = fans
                    .all()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .filter(|f| fan == "all" || f.name == fan || f.index.to_string() == fan)
                    .collect();
                if targets.is_empty() {
                    return Err(format!("no fan named {fan:?}"));
                }
                let mut result = Ok(());
                for target in &targets {
                    match fans.set(target, speed) {
                        Ok(()) => self.hold.set(&target.name, speed),
                        Err(error) => result = result.and(Err(error.to_string())),
                    }
                }
                self.hold.save().map_err(|e| e.to_string())?;
                result?;
            }
            Request::Release => {
                self.hold = Hold::default();
                self.hold.save().map_err(|e| e.to_string())?;
                self.restore().map_err(|e| e.to_string())?;
            }
        }
        Ok(self.hold.to_string())
    }
}
