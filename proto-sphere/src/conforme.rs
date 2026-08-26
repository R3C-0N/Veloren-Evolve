//! La projection conforme du cube sur la sphère.
//!
//! Une face de cube ne peut pas être posée sur la sphère sans déformer ses
//! cases : c'est la taxe de Gauss-Bonnet, et elle se paye soit en forme, soit
//! en taille. La projection équiangulaire garde la taille et laisse les cases
//! devenir des losanges à 120° près des coins. Celle-ci fait le choix inverse :
//! **les cases restent carrées partout**, et c'est leur taille qui varie —
//! jusqu'à un quart au voisinage d'un coin, et zéro au coin même.
//!
//! ## Comment elle est construite
//!
//! En coordonnée stéréographique `ζ` prise depuis le centre de la face, les
//! huit coins du cube sont les racines de
//!
//! ```text
//! Φ(ζ) = ζ⁸ + 14ζ⁴ + 1
//! ```
//!
//! La surface plate du cube n'offre que 270° à chacun de ces coins, contre 360°
//! sur la sphère : la carte doit donc s'y comporter en puissance 3/4. C'est un
//! Schwarz-Christoffel, et il s'écrit d'un trait :
//!
//! ```text
//! dζ/dw = K · Φ(ζ)^(1/4)
//! ```
//!
//! On intègre cette équation depuis le centre de la face — Runge-Kutta 4, une
//! colonne puis chaque ligne — et on tabule le résultat. La table est partagée
//! **octet pour octet** avec le vertex shader : le rendu et la logique doivent
//! voir exactement la même planète, sans quoi la visée dériverait à nouveau.
//!
//! La constante `K` est fixée par la condition que le milieu d'une arête tombe
//! en `w = i`. Deux vérifications tiennent lieu de preuve, et `--diag` les
//! refait : le coin doit atterrir sur `(1+i)/(√3+1)`, et l'angle des côtés
//! d'une case doit valoir 90° partout.

use std::sync::OnceLock;

/// Côté de la table. Impair : le centre de la face doit être un point tabulé.
pub const N: usize = 513;

/// `K = ∫₀^{√2−1} dy / (y⁸ + 14y⁴ + 1)^{1/4}`, l'intégrale le long de l'axe
/// imaginaire jusqu'au milieu d'arête. Calculée une fois, à part.
pub const K: f64 = 0.406_683_250_145_049_9;

type C = (f64, f64);

fn mul(a: C, b: C) -> C { (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0) }
fn add(a: C, b: C) -> C { (a.0 + b.0, a.1 + b.1) }
fn ech(a: C, k: f64) -> C { (a.0 * k, a.1 * k) }

/// `K · Φ(ζ)^{1/4}`, sur la branche principale.
///
/// Elle suffit : sur toute la face, `arg Φ` reste dans `±2,10 rad`, donc bien
/// en deçà de la coupure. C'est vérifié, pas supposé — sans quoi il faudrait
/// suivre la branche pas à pas.
fn derivee(z: C) -> C {
    let z2 = mul(z, z);
    let z4 = mul(z2, z2);
    let z8 = mul(z4, z4);
    let phi = (z8.0 + 14.0 * z4.0 + 1.0, z8.1 + 14.0 * z4.1);

    let module = (phi.0 * phi.0 + phi.1 * phi.1).sqrt().sqrt().sqrt();
    let angle = phi.1.atan2(phi.0) / 4.0;
    (K * module * angle.cos(), K * module * angle.sin())
}

fn pas_rk4(z: C, dw: C) -> C {
    let k1 = derivee(z);
    let k2 = derivee(add(z, ech(mul(dw, k1), 0.5)));
    let k3 = derivee(add(z, ech(mul(dw, k2), 0.5)));
    let k4 = derivee(add(z, mul(dw, k3)));
    let somme = (
        k1.0 + 2.0 * k2.0 + 2.0 * k3.0 + k4.0,
        k1.1 + 2.0 * k2.1 + 2.0 * k3.1 + k4.1,
    );
    add(z, ech(mul(dw, somme), 1.0 / 6.0))
}

pub struct Table {
    /// `ζ` pour chaque `(s, t)` de la grille. En `f32` : c'est exactement ce
    /// que le shader lira, et les deux doivent voir la même chose.
    zeta: Vec<[f32; 2]>,
}

static TABLE: OnceLock<Table> = OnceLock::new();

pub fn table() -> &'static Table { TABLE.get_or_init(Table::construire) }

impl Table {
    fn construire() -> Self {
        let h = 2.0 / (N - 1) as f64;
        let milieu = (N - 1) / 2;
        let mut zeta = vec![[0.0f32; 2]; N * N];
        let pose = |z: &mut Vec<[f32; 2]>, i: usize, j: usize, v: C| {
            z[j * N + i] = [v.0 as f32, v.1 as f32];
        };

        // La colonne centrale, en remontant puis en descendant depuis `ζ = 0`.
        let mut colonne = vec![(0.0, 0.0); N];
        let mut z = (0.0, 0.0);
        for c in colonne.iter_mut().skip(milieu + 1) {
            z = pas_rk4(z, (0.0, h));
            *c = z;
        }
        z = (0.0, 0.0);
        for c in colonne.iter_mut().take(milieu).rev() {
            z = pas_rk4(z, (0.0, -h));
            *c = z;
        }

        // Puis chaque ligne, à partir d'elle.
        for (j, depart) in colonne.iter().enumerate() {
            pose(&mut zeta, milieu, j, *depart);
            let mut z = *depart;
            for i in milieu + 1..N {
                z = pas_rk4(z, (h, 0.0));
                pose(&mut zeta, i, j, z);
            }
            let mut z = *depart;
            for i in (0..milieu).rev() {
                z = pas_rk4(z, (-h, 0.0));
                pose(&mut zeta, i, j, z);
            }
        }

        Self { zeta }
    }

    /// `ζ` interpolé, pour `(s, t)` dans `[−1, 1]²`.
    ///
    /// L'interpolation bilinéaire est reproduite à l'identique dans le shader.
    pub fn zeta(&self, s: f64, t: f64) -> C {
        let n = (N - 1) as f64;
        let x = ((s + 1.0) * 0.5 * n).clamp(0.0, n - 1e-9);
        let y = ((t + 1.0) * 0.5 * n).clamp(0.0, n - 1e-9);
        let (i, j) = (x as usize, y as usize);
        let (fx, fy) = (x - i as f64, y - j as f64);

        let lis = |i: usize, j: usize| {
            let v = self.zeta[j * N + i];
            (v[0] as f64, v[1] as f64)
        };
        let (a, b, c, d) = (lis(i, j), lis(i + 1, j), lis(i, j + 1), lis(i + 1, j + 1));
        let bas = (a.0 + (b.0 - a.0) * fx, a.1 + (b.1 - a.1) * fx);
        let haut = (c.0 + (d.0 - c.0) * fx, c.1 + (d.1 - c.1) * fx);
        (bas.0 + (haut.0 - bas.0) * fy, bas.1 + (haut.1 - bas.1) * fy)
    }

    /// Direction unité dans le repère de la face — `x` le long de `r`, `y` le
    /// long de `h`, `z` le long de la normale.
    pub fn direction_locale(&self, s: f64, t: f64) -> [f64; 3] {
        stereographique_inverse(self.zeta(s, t))
    }

    /// L'inverse, par Newton. La dérivée est connue exactement — c'est l'EDO
    /// elle-même — donc trois ou quatre itérations suffisent.
    ///
    /// Le pas est borné : au voisinage d'un coin, `dw/dζ` diverge, et un Newton
    /// sans garde-fou y partirait à l'infini.
    pub fn depuis_locale(&self, d: [f64; 3]) -> (f64, f64) {
        let cible = (d[0] / (1.0 + d[2]), d[1] / (1.0 + d[2]));

        // Départ : l'estimation équiangulaire, qui n'est jamais très loin.
        let quart = std::f64::consts::FRAC_PI_4;
        let mut w = (
            ((d[0] / d[2]).atan() / quart).clamp(-1.0, 1.0),
            ((d[1] / d[2]).atan() / quart).clamp(-1.0, 1.0),
        );

        for _ in 0..8 {
            let z = self.zeta(w.0, w.1);
            let ecart = (cible.0 - z.0, cible.1 - z.1);
            if ecart.0.abs() + ecart.1.abs() < 1e-12 {
                break;
            }
            // dw = dζ / (K·Φ^{1/4})
            let dz = derivee(z);
            let n2 = dz.0 * dz.0 + dz.1 * dz.1;
            if n2 < 1e-18 {
                break;
            }
            let mut pas = (
                (ecart.0 * dz.0 + ecart.1 * dz.1) / n2,
                (ecart.1 * dz.0 - ecart.0 * dz.1) / n2,
            );
            let long = (pas.0 * pas.0 + pas.1 * pas.1).sqrt();
            if long > 0.25 {
                pas = ech(pas, 0.25 / long);
            }
            w = (
                (w.0 + pas.0).clamp(-1.0, 1.0),
                (w.1 + pas.1).clamp(-1.0, 1.0),
            );
        }
        w
    }

    /// La table telle que le shader la lira : lignes complétées au multiple de
    /// 256 octets qu'exige la copie vers une texture.
    pub fn octets(&self) -> (Vec<u8>, u32) {
        let brut = N * 8;
        let pas = brut.div_ceil(256) * 256;
        let mut octets = vec![0u8; pas * N];
        for j in 0..N {
            for i in 0..N {
                let v = self.zeta[j * N + i];
                let o = j * pas + i * 8;
                octets[o..o + 4].copy_from_slice(&v[0].to_le_bytes());
                octets[o + 4..o + 8].copy_from_slice(&v[1].to_le_bytes());
            }
        }
        (octets, pas as u32)
    }
}

fn stereographique_inverse((p, q): C) -> [f64; 3] {
    let d = 1.0 + p * p + q * q;
    [2.0 * p / d, 2.0 * q / d, (1.0 - p * p - q * q) / d]
}

/// Où doit tomber un coin de face : `(1 + i)/(√3 + 1)`. `--diag` s'en sert pour
/// juger la table plutôt que de la croire sur parole.
pub fn coin_attendu() -> C {
    let c = 1.0 / (3.0f64.sqrt() + 1.0);
    (c, c)
}

/// `ζ` tabulé au coin de la face, pour comparaison.
pub fn coin_mesure() -> C { table().zeta(1.0, 1.0) }
