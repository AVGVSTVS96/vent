use crate::fan::{Fan, Fans, Speed};
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
        self.drive(|held| held)
    }

    /// Gives every held fan back to the system, keeping the hold for later.
    pub fn restore(&self) -> smc::Result<()> {
        self.drive(|_| Speed::Auto)
    }

    fn drive(&self, speed_for: impl Fn(Speed) -> Speed) -> smc::Result<()> {
        let fans = self.fans();
        for fan in fans.all()? {
            if let Some(held) = self.hold.get(&fan.name) {
                let speed = speed_for(held);
                fans.set(&fan, speed)?;
                eprintln!("vent daemon: {} {speed}", fan.name);
            }
        }
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
                for target in &targets {
                    fans.set(target, speed).map_err(|e| e.to_string())?;
                    self.hold.set(&target.name, speed);
                }
                self.hold.save().map_err(|e| e.to_string())?;
            }
            Request::Release => {
                self.restore().map_err(|e| e.to_string())?;
                self.hold = Hold::default();
                self.hold.save().map_err(|e| e.to_string())?;
            }
        }
        Ok(self.hold.to_string())
    }
}
