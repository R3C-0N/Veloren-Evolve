use common_vocab::{
    consts::{AIR_DENSITY, WATER_DENSITY},
    phys::{Density, Mass},
};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use strum::EnumIter;
use vek::*;

// Doesn't include a lot of bodies...
// Intentionally?
pub const ALL_BODIES: [Body; 6] = [
    Body::DefaultAirship,
    Body::AirBalloon,
    Body::SailBoat,
    Body::Galleon,
    Body::Skiff,
    Body::Submarine,
];

pub const ALL_AIRSHIPS: [Body; 2] = [Body::DefaultAirship, Body::AirBalloon];
pub const ALL_SHIPS: [Body; 7] = [
    Body::SailBoat,
    Body::Galleon,
    Body::Skiff,
    Body::Submarine,
    Body::Carriage,
    Body::Cart,
    Body::Train,
];

#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, EnumIter,
)]
#[repr(u32)]
pub enum Body {
    DefaultAirship = 0,
    AirBalloon = 1,
    SailBoat = 2,
    Galleon = 3,
    Volume = 4,
    Skiff = 5,
    Submarine = 6,
    Carriage = 7,
    Cart = 8,
    Train = 9,
}

impl From<Body> for super::Body {
    fn from(body: Body) -> Self { super::Body::Ship(body) }
}

impl Body {
    pub fn random() -> Self {
        let mut rng = rand::rng();
        Self::random_with(&mut rng)
    }

    pub fn random_with(rng: &mut impl rand::RngExt) -> Self { *ALL_BODIES.choose(rng).unwrap() }

    pub fn random_airship_with(rng: &mut impl rand::RngExt) -> Self {
        *ALL_AIRSHIPS.choose(rng).unwrap()
    }

    pub fn random_ship_with(rng: &mut impl rand::RngExt) -> Self { *ALL_SHIPS.choose(rng).unwrap() }

    /// Return the structure manifest that this ship uses. `None` means that it
    /// should be derived from the collider.
    pub fn manifest_entry(&self) -> Option<&'static str> {
        match self {
            Body::DefaultAirship => Some("airship_human.structure"),
            Body::AirBalloon => Some("air_balloon.structure"),
            Body::SailBoat => Some("sail_boat.structure"),
            Body::Galleon => Some("galleon.structure"),
            Body::Skiff => Some("skiff.structure"),
            Body::Submarine => Some("submarine.structure"),
            Body::Carriage => Some("carriage.structure"),
            Body::Cart => Some("cart.structure"),
            Body::Volume => None,
            Body::Train => Some("train.loco"),
        }
    }

    pub fn dimensions(&self) -> Vec3<f32> {
        match self {
            Body::DefaultAirship | Body::Volume => Vec3::new(25.0, 50.0, 40.0),
            Body::AirBalloon => Vec3::new(25.0, 50.0, 40.0),
            Body::SailBoat => Vec3::new(12.0, 32.0, 6.0),
            Body::Galleon => Vec3::new(14.0, 48.0, 10.0),
            Body::Skiff => Vec3::new(7.0, 15.0, 10.0),
            Body::Submarine => Vec3::new(2.0, 15.0, 8.0),
            Body::Carriage => Vec3::new(5.0, 12.0, 2.0),
            Body::Cart => Vec3::new(3.0, 6.0, 1.0),
            Body::Train => Vec3::new(7.0, 32.0, 5.0),
        }
    }

    fn balloon_vol(&self) -> f32 {
        match self {
            Body::DefaultAirship | Body::AirBalloon | Body::Volume => {
                let spheroid_vol = |equat_d: f32, polar_d: f32| -> f32 {
                    (std::f32::consts::PI / 6.0) * equat_d.powi(2) * polar_d
                };
                let dim = self.dimensions();
                spheroid_vol(dim.z, dim.y)
            },
            _ => 0.0,
        }
    }

    fn hull_vol(&self) -> f32 {
        // height from bottom of keel to deck
        let deck_height = 10_f32;
        let dim = self.dimensions();
        (std::f32::consts::PI / 6.0) * (deck_height * 1.5).powi(2) * dim.y
    }

    pub fn hull_density(&self) -> Density {
        let oak_density = 600_f32;
        let ratio = 0.1;
        Density(ratio * oak_density + (1.0 - ratio) * AIR_DENSITY)
    }

    pub fn density(&self) -> Density {
        match self {
            Body::DefaultAirship | Body::AirBalloon | Body::Volume => Density(AIR_DENSITY),
            Body::Submarine => Density(WATER_DENSITY), // Neutrally buoyant
            Body::Carriage => Density(WATER_DENSITY * 0.5),
            Body::Cart => Density(500.0 / self.dimensions().product()), /* Carts get a constant */
            // Trains are heavy as hell
            Body::Train => Density(WATER_DENSITY * 1.5),
            _ => Density(AIR_DENSITY * 0.95 + WATER_DENSITY * 0.05), /* Most boats should be very
                                                                      * buoyant */
        }
    }

    pub fn mass(&self) -> Mass {
        if self.can_fly() {
            Mass((self.hull_vol() + self.balloon_vol()) * self.density().0)
        } else {
            Mass(self.density().0 * self.dimensions().product())
        }
    }

    pub fn can_fly(&self) -> bool {
        matches!(self, Body::DefaultAirship | Body::AirBalloon | Body::Volume)
    }

    pub fn vectored_propulsion(&self) -> bool { matches!(self, Body::DefaultAirship) }

    pub fn flying_height(&self) -> f32 {
        if self.can_fly() {
            match self {
                Body::DefaultAirship => 300.0,
                Body::AirBalloon => 200.0,
                _ => 0.0,
            }
        } else {
            0.0
        }
    }

    pub fn has_water_thrust(&self) -> bool {
        matches!(self, Body::SailBoat | Body::Galleon | Body::Skiff)
    }

    pub fn has_wheels(&self) -> bool { matches!(self, Body::Carriage | Body::Cart | Body::Train) }

    /// Max speed in block/s.
    /// This is the simulated speed of Ship bodies (which are NPCs).
    ///
    /// Air Vehicles:
    /// Loaded (non-simulated) air ships don't have a speed, they have thrust
    /// that produces acceleration and air resistance that produces drag.
    /// The acceleration is modulated by a speed_factor assigned
    /// by the agent, and the balance of forces results in a semi-constant
    /// velocity (speed) when thrust and drag are in equilibrium. The
    /// average velocity changes depending on wind and changes in altitude
    /// (e.g. when terrain following and going up or down over mountains).
    ///
    /// Water Vehicles:
    /// Loaded water ships also have thrust and drag, and a speed_factor that
    /// modulates the resulting acceleration and top speed. Wind does have
    /// an effect on the velocity of watercraft.
    ///
    /// The airship simulated speed below was chosen experimentally so that the
    /// time required to complete one full circuit of an airship multi-leg
    /// route is the same for simulated airships and loaded airships (one
    /// where a player is continuously riding the airship).
    ///
    /// Other vehicles should be tuned if and when implemented, but for now the
    /// airship is the only Ship in use.
    pub fn get_speed(&self) -> f32 {
        match self {
            Body::DefaultAirship => 23.0,
            Body::AirBalloon => 8.0,
            Body::SailBoat => 5.0,
            Body::Galleon => 6.0,
            Body::Skiff => 6.0,
            Body::Submarine => 4.0,
            Body::Train => 12.0,
            _ => 10.0,
        }
    }
}

/// Terrain is 11.0 scale relative to small-scale voxels,
/// airship scale is multiplied by 11 to reach terrain scale.
pub const AIRSHIP_SCALE: f32 = 11.0;

// Les gabarits de voxels sont dans `crate::figure::ship_spec`, et se nomment par
// ce chemin. Les garder ici faisait dependre tout `body` de `terrain` et de
// `figure` ; un alias les y ramenerait par la bande.
