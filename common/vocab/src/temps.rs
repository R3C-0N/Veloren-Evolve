//! Les mesures de temps du jeu.
//!
//! `Time` et `Secs` sont de simples enveloppes autour d'un `f64`, mais elles
//! traversent tout le code. Les garder dans `resources.rs`, qui connait `specs`
//! et les composants, obligeait a dependre de `veloren-common` entier pour
//! nommer une duree.
//!
//! `Time::add_days` reste la-haut : elle a besoin des constantes du serveur.

use serde::{Deserialize, Serialize};
use std::ops::{Mul, MulAssign};

/// A resource that stores the tick (i.e: physics) time.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq, PartialOrd)]
pub struct Time(pub f64);

impl Time {
    pub fn add_seconds(self, seconds: f64) -> Self { Self(self.0 + seconds) }

    pub fn add_minutes(self, minutes: f64) -> Self { Self(self.0 + minutes * 60.0) }
}

/// A resource that stores the real tick, local to the server/client.
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ProgramTime(pub f64);

/// A resource used to indicate a duration of time, in seconds
#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct Secs(pub f64);

impl Mul<f64> for Secs {
    type Output = Self;

    fn mul(self, mult: f64) -> Self { Self(self.0 * mult) }
}
impl MulAssign<f64> for Secs {
    fn mul_assign(&mut self, mult: f64) { *self = *self * mult; }
}
