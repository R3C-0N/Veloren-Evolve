//! Comment un corps se deplace : acceleration, nage, vol, saut, escalade.
//!
//! Ces methodes vivaient dans `states::utils`, cote logique d'etat, mais ce
//! sont des proprietes du corps -- des tables par espece. Les laisser la-haut
//! aurait demande un trait d'extension une fois les corps sortis en crate, pour
//! du code qui n'a besoin de rien d'autre que des especes.

use common_vocab::consts::{FRIC_GROUND, GRAVITY};
use vek::Vec3;

use super::{
    Body, arthropod, biped_large, biped_small, bird_medium, crustacean, golem, object,
    quadruped_low, quadruped_medium, quadruped_small, ship, theropod,
};

impl Body {
    pub fn base_accel(&self) -> f32 {
        match self {
            // Note: Entities have been slowed down relative to humanoid speeds, but it may be worth
            // reverting/increasing speed once we've established slower AI.
            Body::Humanoid(_) => 100.0,
            Body::QuadrupedSmall(body) => match body.species {
                quadruped_small::Species::Turtle => 30.0,
                quadruped_small::Species::Axolotl => 70.0,
                quadruped_small::Species::Pig => 70.0,
                quadruped_small::Species::Sheep => 70.0,
                quadruped_small::Species::Truffler => 70.0,
                quadruped_small::Species::Fungome => 70.0,
                quadruped_small::Species::Goat => 80.0,
                quadruped_small::Species::Raccoon => 100.0,
                quadruped_small::Species::Frog => 150.0,
                quadruped_small::Species::Porcupine => 100.0,
                quadruped_small::Species::Beaver => 100.0,
                quadruped_small::Species::Rabbit => 110.0,
                quadruped_small::Species::Cat => 150.0,
                quadruped_small::Species::Quokka => 100.0,
                quadruped_small::Species::MossySnail => 20.0,
                _ => 125.0,
            },
            Body::QuadrupedMedium(quadruped_medium) => match quadruped_medium.species {
                quadruped_medium::Species::Grolgar => 100.0,
                quadruped_medium::Species::Saber => 110.0,
                quadruped_medium::Species::Tiger => 110.0,
                quadruped_medium::Species::Tuskram => 85.0,
                quadruped_medium::Species::Lion => 105.0,
                quadruped_medium::Species::Tarasque => 100.0,
                quadruped_medium::Species::Wolf => 130.0,
                quadruped_medium::Species::Frostfang => 115.0,
                quadruped_medium::Species::Mouflon => 75.0,
                quadruped_medium::Species::Catoblepas => 60.0,
                quadruped_medium::Species::Bonerattler => 115.0,
                quadruped_medium::Species::Deer => 120.0,
                quadruped_medium::Species::Hirdrasil => 110.0,
                quadruped_medium::Species::Roshwalr => 70.0,
                quadruped_medium::Species::Donkey => 90.0,
                quadruped_medium::Species::Camel => 75.0,
                quadruped_medium::Species::Zebra => 150.0,
                quadruped_medium::Species::Antelope => 155.0,
                quadruped_medium::Species::Kelpie => 140.0,
                quadruped_medium::Species::Horse => 140.0,
                quadruped_medium::Species::Barghest => 80.0,
                quadruped_medium::Species::Cattle => 80.0,
                quadruped_medium::Species::Darkhound => 115.0,
                quadruped_medium::Species::Highland => 80.0,
                quadruped_medium::Species::Yak => 80.0,
                quadruped_medium::Species::Panda => 90.0,
                quadruped_medium::Species::Bear => 90.0,
                quadruped_medium::Species::Dreadhorn => 95.0,
                quadruped_medium::Species::Moose => 105.0,
                quadruped_medium::Species::Snowleopard => 115.0,
                quadruped_medium::Species::Mammoth => 75.0,
                quadruped_medium::Species::Elephant => 75.0,
                quadruped_medium::Species::Ngoubou => 95.0,
                quadruped_medium::Species::Llama => 100.0,
                quadruped_medium::Species::Alpaca => 100.0,
                quadruped_medium::Species::Akhlut => 90.0,
                quadruped_medium::Species::Bristleback => 105.0,
                quadruped_medium::Species::ClaySteed => 85.0,
            },
            Body::BipedLarge(body) => match body.species {
                biped_large::Species::Slysaurok => 100.0,
                biped_large::Species::Occultsaurok => 100.0,
                biped_large::Species::Mightysaurok => 100.0,
                biped_large::Species::Mindflayer => 90.0,
                biped_large::Species::Minotaur => 60.0,
                biped_large::Species::Huskbrute => 130.0,
                biped_large::Species::Cultistwarlord => 110.0,
                biped_large::Species::Cultistwarlock => 90.0,
                biped_large::Species::Gigasfrost => 45.0,
                biped_large::Species::Gigasfire => 50.0,
                biped_large::Species::Forgemaster => 100.0,
                _ => 80.0,
            },
            Body::BirdMedium(_) => 80.0,
            Body::FishMedium(_) => 80.0,
            Body::Dragon(_) => 250.0,
            Body::BirdLarge(_) => 110.0,
            Body::FishSmall(_) => 60.0,
            Body::BipedSmall(biped_small) => match biped_small.species {
                biped_small::Species::Haniwa => 65.0,
                biped_small::Species::Boreal => 100.0,
                biped_small::Species::Gnarling => 70.0,
                _ => 80.0,
            },
            Body::Object(_) => 0.0,
            Body::Item(_) => 0.0,
            Body::Golem(body) => match body.species {
                golem::Species::ClayGolem => 120.0,
                golem::Species::IronGolem => 100.0,
                _ => 60.0,
            },
            Body::Theropod(theropod) => match theropod.species {
                theropod::Species::Archaeos
                | theropod::Species::Odonto
                | theropod::Species::Ntouka => 110.0,
                theropod::Species::Dodarock => 75.0,
                theropod::Species::Yale => 115.0,
                _ => 125.0,
            },
            Body::QuadrupedLow(quadruped_low) => match quadruped_low.species {
                quadruped_low::Species::Crocodile => 60.0,
                quadruped_low::Species::SeaCrocodile => 60.0,
                quadruped_low::Species::Alligator => 65.0,
                quadruped_low::Species::Salamander => 85.0,
                quadruped_low::Species::Elbst => 85.0,
                quadruped_low::Species::Monitor => 130.0,
                quadruped_low::Species::Asp => 100.0,
                quadruped_low::Species::Tortoise => 60.0,
                quadruped_low::Species::Rocksnapper => 70.0,
                quadruped_low::Species::Rootsnapper => 70.0,
                quadruped_low::Species::Reefsnapper => 70.0,
                quadruped_low::Species::Pangolin => 90.0,
                quadruped_low::Species::Maneater => 80.0,
                quadruped_low::Species::Sandshark => 125.0,
                quadruped_low::Species::Hakulaq => 125.0,
                quadruped_low::Species::Dagon => 140.0,
                quadruped_low::Species::Lavadrake => 100.0,
                quadruped_low::Species::Icedrake => 100.0,
                quadruped_low::Species::Basilisk => 85.0,
                quadruped_low::Species::Deadwood => 110.0,
                quadruped_low::Species::Mossdrake => 100.0,
                quadruped_low::Species::Driggle => 120.0,
                quadruped_low::Species::Snaretongue => 120.0,
                quadruped_low::Species::Hydra => 100.0,
            },
            Body::Ship(ship::Body::Carriage) => 40.0,
            Body::Ship(ship::Body::Train) => 9.0,
            Body::Ship(_) => 0.0,
            Body::Arthropod(arthropod) => match arthropod.species {
                arthropod::Species::Tarantula => 85.0,
                arthropod::Species::Blackwidow => 95.0,
                arthropod::Species::Antlion => 115.0,
                arthropod::Species::Hornbeetle => 80.0,
                arthropod::Species::Leafbeetle => 65.0,
                arthropod::Species::Stagbeetle => 80.0,
                arthropod::Species::Weevil => 70.0,
                arthropod::Species::Cavespider => 90.0,
                arthropod::Species::Moltencrawler => 70.0,
                arthropod::Species::Mosscrawler => 70.0,
                arthropod::Species::Sandcrawler => 70.0,
                arthropod::Species::Dagonite => 70.0,
                arthropod::Species::Emberfly => 75.0,
            },
            Body::Crustacean(body) => match body.species {
                crustacean::Species::Crab | crustacean::Species::SoldierCrab => 80.0,
                crustacean::Species::Karkatha => 120.0,
            },
            Body::Plugin(body) => body.base_accel(),
        }
    }

    pub fn air_accel(&self) -> f32 { self.base_accel() * 0.025 }

    /// Attempt to determine the maximum speed of the character
    /// when moving on the ground
    pub fn max_speed_approx(&self) -> f32 {
        let v = match self {
            Body::Ship(ship) => ship.get_speed(),
            // NOTE: that denominator evaluates to constant, at the time
            // of writing it's ~9.751134.
            //
            // We still have the formula here, for the sake of completeness,
            // and also for when we'll split FRIC_GROUND to be different
            // on the snow/ice/etc.
            _ => -self.base_accel() / (60.0 * (1.0 - FRIC_GROUND).ln()),
        };
        debug_assert!(v >= 0.0, "Speed must be positive!");
        v
    }

    /// How much orientation changes will be damped based on the severity of the
    /// turn.
    ///
    /// At 1.0, low-severity turns will be damped to a lower rate: this is more
    /// typical of the way bipedal creatures turn, for example. At 0.0, the
    /// turn rate is constant regardless of angle.
    pub fn ori_damping(&self) -> f32 {
        match self {
            Body::Humanoid(_) | Body::BipedLarge(_) | Body::Golem(_) => 1.0,
            _ => 0.0,
        }
    }

    /// The turn rate in 180°/s (or (rotations per second)/2)
    pub fn base_ori_rate(&self) -> f32 {
        match self {
            Body::Humanoid(_) => 2.65,
            Body::QuadrupedSmall(_) => 3.0,
            Body::QuadrupedMedium(quadruped_medium) => match quadruped_medium.species {
                quadruped_medium::Species::Mammoth => 1.0,
                _ => 2.8,
            },
            Body::BirdMedium(_) => 6.0,
            Body::FishMedium(_) => 6.0,
            Body::Dragon(_) => 1.0,
            Body::BirdLarge(_) => 7.0,
            Body::FishSmall(_) => 7.0,
            Body::BipedLarge(biped_large) => match biped_large.species {
                biped_large::Species::Harvester => 2.0,
                _ => 2.7,
            },
            Body::BipedSmall(_) => 3.5,
            Body::Object(_) => 2.0,
            Body::Item(_) => 2.0,
            Body::Golem(golem) => match golem.species {
                golem::Species::WoodGolem => 1.2,
                _ => 2.0,
            },
            Body::Theropod(theropod) => match theropod.species {
                theropod::Species::Archaeos => 2.3,
                theropod::Species::Odonto => 2.3,
                theropod::Species::Ntouka => 2.3,
                theropod::Species::Dodarock => 2.0,
                _ => 2.5,
            },
            Body::QuadrupedLow(quadruped_low) => match quadruped_low.species {
                quadruped_low::Species::Asp => 2.2,
                quadruped_low::Species::Tortoise => 1.5,
                quadruped_low::Species::Rocksnapper => 1.8,
                quadruped_low::Species::Rootsnapper => 1.8,
                quadruped_low::Species::Lavadrake => 1.7,
                quadruped_low::Species::Icedrake => 1.7,
                quadruped_low::Species::Mossdrake => 1.7,
                _ => 2.0,
            },
            Body::Ship(ship::Body::Carriage) => 0.04,
            Body::Ship(ship::Body::Train) => 0.0,
            Body::Ship(ship) if ship.has_water_thrust() => 5.0 / self.dimensions().y,
            Body::Ship(_) => 6.0 / self.dimensions().y,
            Body::Arthropod(_) => 3.5,
            Body::Crustacean(_) => 3.5,
            Body::Plugin(body) => body.base_ori_rate(),
        }
    }

    /// Returns thrust force if the body type can swim, otherwise None
    pub fn swim_thrust(&self) -> Option<f32> {
        // Swim thrust is proportional to the frontal area of the creature, since we
        // assume that strength roughly scales according to square laws. Also,
        // it happens to make balancing against drag much simpler.
        let front_profile = self.dimensions().x * self.dimensions().z;
        Some(
            match self {
                Body::Object(_) => return None,
                Body::Item(_) => return None,
                Body::Ship(ship::Body::Submarine) => 1000.0 * self.mass().0,
                Body::Ship(ship) if ship.has_water_thrust() => 500.0 * self.mass().0,
                Body::Ship(_) => return None,
                Body::BipedLarge(_) => 120.0 * self.mass().0,
                Body::Golem(_) => 100.0 * self.mass().0,
                Body::BipedSmall(_) => 1000.0 * self.mass().0,
                Body::BirdMedium(_) => 400.0 * self.mass().0,
                Body::BirdLarge(_) => 400.0 * self.mass().0,
                Body::FishMedium(_) => 200.0 * self.mass().0,
                Body::FishSmall(_) => 300.0 * self.mass().0,
                Body::Dragon(_) => 50.0 * self.mass().0,
                // Humanoids are a bit different: we try to give them thrusts that result in similar
                // speeds for gameplay reasons
                Body::Humanoid(body) => {
                    return Some(6_500_000.0 / self.mass().0 * body.scaler().powi(2));
                },
                Body::Theropod(body) => match body.species {
                    theropod::Species::Sandraptor
                    | theropod::Species::Snowraptor
                    | theropod::Species::Sunlizard
                    | theropod::Species::Woodraptor
                    | theropod::Species::Dodarock
                    | theropod::Species::Axebeak
                    | theropod::Species::Yale => 500.0 * self.mass().0,
                    _ => 150.0 * self.mass().0,
                },
                Body::QuadrupedLow(_) => 1200.0 * self.mass().0,
                Body::QuadrupedMedium(body) => match body.species {
                    quadruped_medium::Species::Mammoth => 150.0 * self.mass().0,
                    quadruped_medium::Species::Kelpie => 3500.0 * self.mass().0,
                    _ => 1000.0 * self.mass().0,
                },
                Body::QuadrupedSmall(_) => 1500.0 * self.mass().0,
                Body::Arthropod(_) => 500.0 * self.mass().0,
                Body::Crustacean(_) => 400.0 * self.mass().0,
                Body::Plugin(body) => body.swim_thrust()?,
            } * front_profile,
        )
    }

    /// Returns thrust force if the body type can fly, otherwise None
    pub fn fly_thrust(&self) -> Option<f32> {
        match self {
            Body::BirdMedium(body) => match body.species {
                bird_medium::Species::Bat | bird_medium::Species::BloodmoonBat => {
                    Some(GRAVITY * self.mass().0 * 0.5)
                },
                _ => Some(GRAVITY * self.mass().0 * 2.0),
            },
            Body::BirdLarge(_) => Some(GRAVITY * self.mass().0 * 0.5),
            Body::Dragon(_) => Some(200_000.0),
            Body::Ship(ship) if ship.can_fly() => Some(390_000.0),
            Body::Object(object::Body::Crux) => Some(1_000.0),
            _ => None,
        }
    }

    /// Returns whether the body uses vectored propulsion
    pub fn vectored_propulsion(&self) -> bool {
        match self {
            Body::Ship(ship) => ship.vectored_propulsion(),
            _ => false,
        }
    }

    /// Returns jump impulse if the body type can jump, otherwise None
    pub fn jump_impulse(&self) -> Option<f32> {
        match self {
            Body::Object(_) | Body::Ship(_) | Body::Item(_) => None,
            Body::BipedLarge(_) | Body::Dragon(_) => Some(0.6 * self.mass().0),
            Body::Golem(_) | Body::QuadrupedLow(_) => Some(0.4 * self.mass().0),
            Body::QuadrupedMedium(_) => Some(0.4 * self.mass().0),
            Body::Theropod(body) => match body.species {
                theropod::Species::Snowraptor
                | theropod::Species::Sandraptor
                | theropod::Species::Woodraptor => Some(0.4 * self.mass().0),
                _ => None,
            },
            Body::Arthropod(_) => Some(1.0 * self.mass().0),
            _ => Some(0.4 * self.mass().0),
        }
        .map(|f| f * GRAVITY)
    }

    pub fn can_climb(&self) -> bool { matches!(self, Body::Humanoid(_)) }

    /// Returns how well a body can move backwards while strafing (0.0 = not at
    /// all, 1.0 = same as forward)
    pub fn reverse_move_factor(&self) -> f32 { 0.45 }

    /// Returns the position where a projectile should be fired relative to this
    /// body
    pub fn projectile_offsets(&self, ori: Vec3<f32>, scale: f32) -> Vec3<f32> {
        let body_offsets_z = match self {
            Body::Golem(_) => self.height() * 0.4,
            _ => self.eye_height(scale),
        };

        let dim = self.dimensions();
        // The width (shoulder to shoulder) and length (nose to tail)
        let (width, length) = (dim.x, dim.y);
        let body_radius = if length > width {
            // Dachshund-like
            self.max_radius()
        } else {
            // Cyclops-like
            self.min_radius()
        };

        Vec3::new(
            body_radius * ori.x * 1.1,
            body_radius * ori.y * 1.1,
            body_offsets_z,
        )
    }
}
