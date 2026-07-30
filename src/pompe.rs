use std::time::{Duration, Instant};

/// Anti-claquement : nombre de transitions max autorisées dans la fenêtre glissante ci-dessous.
/// Fenêtre volontairement large (au-delà des rebonds de relais classiques, de l'ordre de la
/// seconde) pour intercepter aussi un cyclage plus lent (dizaines de secondes entre transitions),
/// comme celui observé lors d'une variation rapide de température (ex. chute lors d'un orage).
const CLAQUEMENT_MAX: u32 = 4;
const FENETRE_CLAQUEMENT: Duration = Duration::from_secs(120);
/// Durée du blocage pompe après détection d'un claquement (protège le relais/moteur).
const DUREE_BLOCAGE: Duration = Duration::from_secs(300);

/// Pilotage sécurisé du relais pompe : gating sécurité, anti-claquement (protection
/// matérielle du relais/moteur), cumul du temps de marche du jour. Port du `PumpManager`
/// C++, sans encore la logique automatique (température, plages horaires) — prévue pour
/// une prochaine étape dans ce même fichier.
///
/// Le feedback matériel (ex-relais miroir sur GPIO33) a été retiré : jugé peu fiable
/// (ne détecte pas une panne du relais principal lui-même), remplacé plus tard par un
/// capteur de courant sur l'alimentation de la pompe.
pub struct GestionPompe {
    etat: bool, // état réel actuellement appliqué au relais
    compteur_claquements: u32,
    fenetre_depuis: Instant,
    bloquee_jusqua: Option<Instant>,
    heures_aujourdhui: f32,
    sessions_aujourdhui: u32,
    dernier_tick: Option<Instant>,
}

impl GestionPompe {
    pub fn nouveau() -> Self {
        let maintenant = Instant::now();
        GestionPompe {
            etat: false,
            compteur_claquements: 0,
            fenetre_depuis: maintenant,
            bloquee_jusqua: None,
            heures_aujourdhui: 0.0,
            sessions_aujourdhui: 0,
            dernier_tick: None,
        }
    }

    /// `demande` : intention voulue (marche/arrêt) — déjà filtrée par l'appelant selon le mode
    /// (pour l'instant, marche forcée possible uniquement en MANU ; AUTO = toujours `false`
    /// tant que la logique température/plage horaire n'est pas implémentée).
    /// `systeme_sur` : `false` si une sécurité (niveau d'eau, défaut moteur) interdit la marche.
    pub fn mettre_a_jour(&mut self, demande: bool, systeme_sur: bool) {
        let maintenant = Instant::now();

        // Cumul du temps de marche du jour, calculé sur l'écart depuis le dernier appel.
        let delta = self
            .dernier_tick
            .map(|t| maintenant.duration_since(t))
            .unwrap_or_default();
        self.dernier_tick = Some(maintenant);
        if self.etat {
            self.heures_aujourdhui += delta.as_secs_f32() / 3600.0;
        }

        // Blocage anti-claquement en cours ?
        if let Some(fin_blocage) = self.bloquee_jusqua {
            if maintenant < fin_blocage {
                self.forcer_arret();
                return;
            }
            self.bloquee_jusqua = None;
        }

        if !systeme_sur {
            self.forcer_arret();
            return;
        }

        if demande != self.etat {
            // Anti-claquement : compte les transitions dans la fenêtre glissante.
            if maintenant.duration_since(self.fenetre_depuis) > FENETRE_CLAQUEMENT {
                self.fenetre_depuis = maintenant;
                self.compteur_claquements = 0;
            }
            self.compteur_claquements += 1;
            if self.compteur_claquements >= CLAQUEMENT_MAX {
                self.bloquee_jusqua = Some(maintenant + DUREE_BLOCAGE);
                self.compteur_claquements = 0;
                self.forcer_arret();
                return;
            }

            if demande && !self.etat {
                self.sessions_aujourdhui += 1;
            }
            self.etat = demande;
        }
    }

    fn forcer_arret(&mut self) {
        self.etat = false;
    }

    /// État réel actuellement appliqué (ou à appliquer) au relais.
    pub fn etat(&self) -> bool {
        self.etat
    }

    /// `true` si la pompe est actuellement bloquée suite à un claquement détecté.
    pub fn bloquee(&self) -> bool {
        self.bloquee_jusqua.is_some_and(|fin| Instant::now() < fin)
    }

    /// Temps de marche cumulé depuis la création (à remettre à zéro chaque jour par l'appelant).
    pub fn heures_aujourdhui(&self) -> f32 {
        self.heures_aujourdhui
    }

    /// Nombre de démarrages de la pompe depuis la création (à remettre à zéro chaque jour).
    pub fn sessions_aujourdhui(&self) -> u32 {
        self.sessions_aujourdhui
    }

    /// Remise à zéro du compteur journalier (à appeler par l'appelant au changement de jour).
    pub fn reinitialiser_jour(&mut self) {
        self.heures_aujourdhui = 0.0;
        self.sessions_aujourdhui = 0;
    }

    /// Restaure les compteurs du jour rechargés depuis la NVS après un
    /// redémarrage (voir `journal::charger_pompe_jour`) — seulement si les
    /// valeurs sauvegardées appartiennent bien à la journée en cours.
    pub fn charger_etat_jour(&mut self, heures: f32, sessions: u32) {
        self.heures_aujourdhui = heures;
        self.sessions_aujourdhui = sessions;
    }
}
