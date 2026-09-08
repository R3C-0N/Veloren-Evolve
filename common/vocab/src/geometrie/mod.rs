//! Les types geometriques du jeu : direction, plan, projection, orientation.
//!
//! Ils vivaient dans `util` et `comp::ori`, et se tenaient mutuellement --
//! `Ori` est bati sur `Dir`, et `Dir` savait se convertir en `Ori`. Ce cycle
//! interdisait de les separer, et leur presence dans `veloren-common` obligeait
//! `comp::body` a dependre de tout le jeu pour nommer une orientation.

pub mod dir;
pub mod ori;
pub mod plane;
pub mod projection;

pub use dir::Dir;
pub use ori::Ori;
pub use plane::Plane;
pub use projection::Projection;
