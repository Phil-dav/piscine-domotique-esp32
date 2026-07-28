use std::time::{Duration, Instant};

pub const DUREE_MIN_MINUTES: u32 = 30;
pub const DUREE_MAX_MINUTES: u32 = 480;
pub const DUREE_PAS_MINUTES: u32 = 30;
const DUREE_DEFAUT_MINUTES: u32 = 60;

/// Marche/arrêt forcé temporaire : outrepasse la décision automatique pendant une
/// durée réglable (30 min à 8h, par paliers de 30 min), puis rend la main automatiquement
/// à l'expiration. Disponible uniquement en mode AUTO (vérifié par l'appelant).
pub struct Boost {
    fin: Option<Instant>,
    marche_forcee: bool, // true = force la marche, false = force l'arrêt
    duree_minutes: u32,
}

impl Boost {
    pub fn nouveau(duree_minutes_initiale: u32) -> Self {
        Boost {
            fin: None,
            marche_forcee: true,
            duree_minutes: if duree_minutes_initiale == 0 {
                DUREE_DEFAUT_MINUTES
            } else {
                duree_minutes_initiale
            },
        }
    }

    pub fn actif(&self) -> bool {
        self.fin.is_some_and(|f| Instant::now() < f)
    }

    /// Sens du boost en cours : `true` = force la marche, `false` = force l'arrêt.
    /// N'a de sens que si `actif()` est vrai.
    pub fn marche_forcee(&self) -> bool {
        self.marche_forcee
    }

    pub fn restant_secondes(&self) -> u32 {
        self.fin
            .and_then(|f| f.checked_duration_since(Instant::now()))
            .map(|d| d.as_secs() as u32)
            .unwrap_or(0)
    }

    pub fn duree_minutes(&self) -> u32 {
        self.duree_minutes
    }

    /// Démarre un boost. Le sens (marche/arrêt forcé) est l'inverse de l'état courant de la pompe.
    pub fn demarrer(&mut self, pompe_actuellement_active: bool) {
        self.marche_forcee = !pompe_actuellement_active;
        self.fin = Some(Instant::now() + Duration::from_secs(self.duree_minutes as u64 * 60));
    }

    pub fn arreter(&mut self) {
        self.fin = None;
    }

    /// Règle la durée du prochain boost. Renvoie `false` si invalide (doit être un
    /// multiple de 30 min, entre 30 min et 8h) — la durée n'est alors pas modifiée.
    pub fn definir_duree(&mut self, minutes: u32) -> bool {
        if !(DUREE_MIN_MINUTES..=DUREE_MAX_MINUTES).contains(&minutes)
            || minutes % DUREE_PAS_MINUTES != 0
        {
            return false;
        }
        self.duree_minutes = minutes;
        true
    }
}
