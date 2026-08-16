//! Chien de garde réseau : vérifie que la carte est réellement **joignable**,
//! et pas seulement « associée » au point d'accès Wi-Fi.
//!
//! Contexte — incident du 08/08/2026. `is_connected()` d'esp-idf-svc ne reflète
//! que l'association 802.11 : c'est un simple drapeau alimenté par les
//! événements `STA_CONNECTED` / `STA_DISCONNECTED`, il ne prouve à aucun moment
//! qu'un paquet arrive à destination. Ce jour-là la carte est restée 1h30
//! associée, avec une IP valide (`sta ip: 192.168.1.20` dans le journal), tout
//! en étant totalement injoignable : `getaddrinfo() returns 202` côté Adafruit
//! IO, `ping` sans réponse depuis le PC (« hôte de destination injoignable »,
//! donc échec dès la résolution ARP) et dashboard mort.
//!
//! Le piège est que la logique de reconnexion de `main.rs` est gardée par
//! `if wifi_connecte { ... } else { ...reconnexion... }` : tant que
//! `is_connected()` répond `true`, **aucun mécanisme de secours ne peut se
//! déclencher**. Seul un redémarrage manuel a rétabli le service.
//!
//! D'où cette sonde active : un ping périodique de la passerelle (la box) est
//! le seul moyen de distinguer « associé et fonctionnel » de « associé et
//! mort ». Le ping vise la passerelle et pas un serveur sur Internet : on veut
//! tester le lien local, pas la disponibilité d'un service distant.
//!
//! La sonde tourne dans un **fil dédié** : `EspPing::ping` est bloquant, et un
//! appel réseau bloquant dans la boucle principale a déjà provoqué de vrais
//! redémarrages (voir la mémoire `projet-piege-appels-reseau-bloquants`).
//!
//! ## Révision du 09/08/2026 — l'angle mort du lien « agonisant »
//!
//! La première version déclenchait sur « **aucun** ping réussi depuis 15 min ».
//! Ce critère ne couvrait que le cas du lien totalement mort. Dans la nuit du 08
//! au 09/08, la liaison a perdu **75 % de ses paquets** pendant 1 h 40, dashboard
//! inaccessible depuis le PC et le téléphone — sans que l'escalade se déclenche
//! une seule fois. La raison est arithmétique : avec 75 % de perte et 2 paquets
//! par sondage, la probabilité qu'au moins un passe vaut 1 − 0,75² = **44 %**.
//! Un sondage sur deux réussissait, le compteur repartait de zéro sans cesse, et
//! il a fallu attendre que la liaison meure complètement (5 h 30) pour que le
//! redémarrage automatique intervienne.
//!
//! D'où la mesure d'un **taux de réussite sur une fenêtre glissante** plutôt
//! qu'un simple « depuis quand aucun succès ». Un lien qui perd les trois quarts
//! de ses paquets est inutilisable, même s'il n'est pas mort.

use std::collections::VecDeque;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use esp_idf_svc::ping::{Configuration as ConfigPing, EspPing};
use log::{info, warn};
use parking_lot::Mutex;

/// Intervalle entre deux sondages. Assez espacé pour rester négligeable face au
/// trafic du dashboard, assez fréquent pour détecter un blocage en quelques
/// minutes plutôt qu'en une heure et demie.
const INTERVALLE_SONDE: Duration = Duration::from_secs(60);

/// Nombre de paquets par sondage. Porté de 2 à 4 le 09/08/2026 : avec seulement
/// 2 paquets on ne mesurait pas un taux de perte, on répondait à une question
/// binaire (« au moins un est-il passé ? »). Quatre paquets donnent une estimation
/// exploitable du taux de perte réel.
const PAQUETS_PAR_SONDE: u32 = 4;

/// Taille de la fenêtre glissante, en nombre de sondages. À raison d'un sondage
/// par minute, cela représente 10 minutes d'historique, soit 40 paquets — assez
/// pour distinguer une perte occasionnelle d'une liaison réellement dégradée.
const TAILLE_FENETRE: usize = 10;

/// `esp_ping` crée sa propre tâche FreeRTOS pour l'émission ICMP ; ce fil-ci ne
/// fait qu'attendre le résultat, il n'a donc pas besoin d'une grande pile.
const TAILLE_PILE: usize = 6144;

/// Poignée vers le fil de sondage. Toutes les méthodes sont non bloquantes.
pub struct Sonde {
    partage: Arc<Partage>,
}

/// Résultat cumulé de la fenêtre glissante.
pub struct Bilan {
    /// Paquets émis sur l'ensemble de la fenêtre.
    pub envoyes: u32,
    /// Paquets revenus.
    pub recus: u32,
}

impl Bilan {
    /// Taux de réussite entre 0.0 (rien ne passe) et 1.0 (tout passe).
    pub fn taux_reussite(&self) -> f32 {
        if self.envoyes == 0 {
            return 0.0;
        }
        self.recus as f32 / self.envoyes as f32
    }
}

struct Partage {
    /// Passerelle à sonder. Fournie par la boucle principale à partir de la
    /// configuration IP réelle plutôt que codée en dur : elle change si la
    /// carte est déplacée sur un autre réseau.
    passerelle: Mutex<Option<Ipv4Addr>>,
    /// Instant du dernier sondage réussi. `None` tant qu'aucun n'a abouti.
    dernier_succes: Mutex<Option<Instant>>,
    /// Fenêtre glissante des derniers sondages, sous forme (émis, reçus).
    /// Le plus ancien est retiré dès que `TAILLE_FENETRE` est dépassée.
    fenetre: Mutex<VecDeque<(u32, u32)>>,
}

impl Sonde {
    /// Démarre le fil de sondage. À appeler une seule fois, au démarrage.
    /// Tant qu'aucune passerelle n'est renseignée, le fil ne fait rien.
    pub fn demarrer() -> anyhow::Result<Self> {
        let partage = Arc::new(Partage {
            passerelle: Mutex::new(None),
            dernier_succes: Mutex::new(None),
            fenetre: Mutex::new(VecDeque::with_capacity(TAILLE_FENETRE)),
        });

        let pour_fil = partage.clone();
        std::thread::Builder::new()
            .name("sonde_reseau".into())
            .stack_size(TAILLE_PILE)
            .spawn(move || boucle_sonde(pour_fil))?;

        Ok(Sonde { partage })
    }

    /// Renseigne (ou met à jour) la passerelle à surveiller.
    pub fn definir_passerelle(&self, passerelle: Ipv4Addr) {
        let mut courante = self.partage.passerelle.lock();
        if *courante != Some(passerelle) {
            info!("Sonde réseau : surveillance de la passerelle {passerelle}");
            *courante = Some(passerelle);
        }
    }

    /// Temps écoulé depuis le dernier sondage réussi.
    ///
    /// `None` signifie qu'**aucun** sondage n'a jamais abouti depuis le
    /// démarrage. C'est volontairement distinct de « ça fait longtemps » :
    /// l'appelant doit s'en servir comme garde-fou (une passerelle qui refuse
    /// l'ICMP ne doit surtout pas déclencher de redémarrages en boucle).
    pub fn depuis_dernier_succes(&self) -> Option<Duration> {
        self.partage.dernier_succes.lock().map(|t| t.elapsed())
    }

    /// Bilan de la fenêtre glissante, ou `None` tant qu'elle n'est pas complète.
    ///
    /// Attendre qu'elle soit pleine évite de juger sur un ou deux sondages : au
    /// démarrage, ou juste après une reconnexion, quelques paquets perdus sont
    /// normaux et ne doivent pas déclencher d'escalade.
    pub fn bilan(&self) -> Option<Bilan> {
        let fenetre = self.partage.fenetre.lock();
        if fenetre.len() < TAILLE_FENETRE {
            return None;
        }
        let (envoyes, recus) = fenetre
            .iter()
            .fold((0, 0), |(e, r), (env, rec)| (e + env, r + rec));
        Some(Bilan { envoyes, recus })
    }

    /// Vide la fenêtre. À appeler après une action corrective (réinitialisation
    /// du pilote Wi-Fi) : sans ça, l'historique dégradé d'avant l'action
    /// continuerait de peser et provoquerait une seconde escalade immédiate,
    /// alors qu'il faut laisser à la liaison le temps de se rétablir.
    pub fn reinitialiser_fenetre(&self) {
        self.partage.fenetre.lock().clear();
    }
}

/// Taux de réussite en dessous duquel la liaison est jugée inutilisable. À 0,40, il faut
/// perdre plus de 60 % des paquets pour déclencher — une perte occasionnelle reste sans
/// effet, alors que les épisodes constatés (75 à 100 % de perte) sont largement couverts.
const SEUIL_TAUX_REUSSITE: f32 = 0.40;

/// Délais d'escalade, comptés depuis le début de l'indisponibilité.
/// Durée pendant laquelle le réseau doit être **redevenu utilisable sans interruption**
/// avant qu'on oublie l'indisponibilité en cours.
///
/// Sans cette condition, une liaison qui bat — alternance rapide entre association perdue
/// et retrouvée — remettrait le chronomètre à zéro à chaque retour et n'atteindrait jamais
/// le seuil de redémarrage. C'est exactement ce qui s'est produit dans la nuit du
/// 16/08/2026, où la carte a alterné 1576 relevés déconnectés contre 1161 connectés
/// pendant 1 h 30 : un retour de quelques secondes suffisait à tout annuler.
const STABILITE_AVANT_OUBLI: Duration = Duration::from_secs(60);

const AVANT_RECONNEXION: Duration = Duration::from_secs(30);
const INTERVALLE_RECONNEXION: Duration = Duration::from_secs(30);
const AVANT_RESET_PILOTE: Duration = Duration::from_secs(5 * 60);
const AVANT_REDEMARRAGE: Duration = Duration::from_secs(10 * 60);

/// Ce que la boucle principale doit faire. Le superviseur **décide**, `main.rs` **agit** :
/// ça évite d'emprunter l'objet Wi-Fi ici, et garde la logique isolée et lisible.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    Rien,
    /// Tentative douce : `connect()`, répétée tant que ça ne revient pas.
    Reconnecter {
        minutes: u64,
    },
    /// Arrêt/relance du pilote Wi-Fi. **Une seule fois par épisode** : l'incident du
    /// 16/08/2026 a montré que dix tentatives successives échouaient toutes, et que seul
    /// le redémarrage complet rétablissait la liaison.
    ReinitialiserPilote {
        minutes: u64,
        perte: u32,
    },
    /// Dernier recours : `esp_restart()`.
    Redemarrer {
        minutes: u64,
        perte: u32,
    },
}

/// Superviseur de santé réseau — machine à états unique.
///
/// ## Pourquoi il remplace deux mécanismes
///
/// Jusqu'au 16/08/2026, `main.rs` contenait **deux** blocs qui se recouvraient : la
/// reconnexion (déclenchée par la perte d'association) et le chien de garde « zombie »
/// (déclenché par le taux de perte). Ils avaient des critères, des compteurs et des seuils
/// différents, et appelaient le même remède.
///
/// L'incident de la nuit du 16/08 en a montré les limites : **1 h 30 pour récupérer** au
/// lieu des 25 minutes prévues, pour deux raisons cumulées.
///
/// - Le bloc zombie n'était évalué que si l'association était présente. Or elle a été
///   perdue plus de la moitié du temps (1576 relevés à `false` contre 1161 à `true`) :
///   pendant ces périodes, **rien ne comptait**.
/// - Chaque réinitialisation du pilote remettait le chronomètre de dégradation à zéro, si
///   bien que le seuil de redémarrage n'était jamais atteint.
///
/// ## Le principe retenu
///
/// Une seule notion — **le réseau est-il utilisable ?** — et un seul chronomètre, remis à
/// zéro **uniquement** quand le réseau redevient réellement utilisable, jamais par une
/// action corrective. L'escalade ne dépend plus que de la durée écoulée.
pub struct Superviseur {
    inutilisable_depuis: Option<Instant>,
    /// Depuis quand le réseau est redevenu utilisable, pour appliquer
    /// `STABILITE_AVANT_OUBLI` avant d'effacer une indisponibilité en cours.
    utilisable_depuis: Option<Instant>,
    derniere_reconnexion: Option<Instant>,
    pilote_deja_reinitialise: bool,
    etait_connecte: bool,
}

impl Superviseur {
    pub fn nouveau() -> Self {
        Superviseur {
            inutilisable_depuis: None,
            utilisable_depuis: None,
            derniere_reconnexion: None,
            pilote_deja_reinitialise: false,
            etait_connecte: true,
        }
    }

    /// À appeler à chaque tour de boucle. `sonde` vaut `None` si le Wi-Fi est désactivé.
    pub fn evaluer(
        &mut self,
        wifi_connecte: bool,
        sonde: Option<&Sonde>,
        maintenant: Instant,
    ) -> Action {
        // Au retour de l'association, on repart d'une fenêtre vierge : celle d'avant est
        // pleine des échecs de la coupure, et pèserait encore dix minutes sur le verdict
        // alors que la liaison vient peut-être d'être rétablie.
        if wifi_connecte && !self.etait_connecte {
            if let Some(s) = sonde {
                s.reinitialiser_fenetre();
            }
        }
        self.etait_connecte = wifi_connecte;

        // Utilisabilité. L'association perdue compte immédiatement, sans attendre la
        // fenêtre — c'est précisément ce qui manquait le 16/08. Une fenêtre incomplète
        // vaut bénéfice du doute : on ne juge pas sur un ou deux sondages.
        let (utilisable, perte) = match (wifi_connecte, sonde) {
            (false, _) => (false, 100),
            (true, None) => (true, 0),
            (true, Some(s)) => match s.bilan() {
                None => (true, 0),
                Some(bilan) => {
                    let taux = bilan.taux_reussite();
                    (
                        taux > SEUIL_TAUX_REUSSITE,
                        ((1.0 - taux) * 100.0).round() as u32,
                    )
                }
            },
        };

        if utilisable {
            // On n'efface une indisponibilité en cours qu'après une période de stabilité :
            // un simple aller-retour de quelques secondes ne doit pas annuler le
            // chronomètre, sous peine de ne jamais atteindre le seuil de redémarrage sur
            // une liaison qui bat.
            let stable_depuis = *self.utilisable_depuis.get_or_insert(maintenant);
            if maintenant.duration_since(stable_depuis) >= STABILITE_AVANT_OUBLI {
                self.inutilisable_depuis = None;
                self.derniere_reconnexion = None;
                self.pilote_deja_reinitialise = false;
            }
            return Action::Rien;
        }

        self.utilisable_depuis = None;

        // Garde-fou : tant qu'aucun ping n'a jamais abouti depuis le démarrage, on ne fait
        // rien. Une passerelle qui ignore l'ICMP ne doit pas provoquer de redémarrages en
        // chaîne, et un réseau qui n'a jamais fonctionné ne sera pas réparé par un reboot.
        if sonde.is_none_or(|s| s.depuis_dernier_succes().is_none()) {
            return Action::Rien;
        }

        let depuis = *self.inutilisable_depuis.get_or_insert(maintenant);
        let duree = maintenant.duration_since(depuis);
        let minutes = duree.as_secs() / 60;

        if duree >= AVANT_REDEMARRAGE {
            return Action::Redemarrer { minutes, perte };
        }

        if duree >= AVANT_RESET_PILOTE && !self.pilote_deja_reinitialise {
            self.pilote_deja_reinitialise = true;
            return Action::ReinitialiserPilote { minutes, perte };
        }

        if duree >= AVANT_RECONNEXION
            && self
                .derniere_reconnexion
                .is_none_or(|t| maintenant.duration_since(t) >= INTERVALLE_RECONNEXION)
        {
            self.derniere_reconnexion = Some(maintenant);
            return Action::Reconnecter { minutes };
        }

        Action::Rien
    }
}

fn boucle_sonde(partage: Arc<Partage>) {
    // `esp-idf-svc` journalise chaque étape d'un ping en `info!` (une demi-douzaine
    // de lignes par sondage). Ce serait noyer les journaux que l'utilisateur relit
    // pour diagnostiquer. Best-effort : si le filtre ne prend pas, on récolte du
    // bruit dans les logs, jamais un défaut de fonctionnement.
    unsafe {
        esp_idf_svc::sys::esp_log_level_set(
            c"esp_idf_svc::ping".as_ptr(),
            esp_idf_svc::sys::esp_log_level_t_ESP_LOG_WARN,
        );
    }

    let conf = ConfigPing {
        count: PAQUETS_PAR_SONDE,
        interval: Duration::from_millis(500),
        timeout: Duration::from_secs(2),
        ..Default::default()
    };

    loop {
        std::thread::sleep(INTERVALLE_SONDE);

        // Le verrou est relâché avant le ping (qui bloque plusieurs secondes) :
        // `definir_passerelle` ne doit jamais attendre après la boucle principale.
        let passerelle = *partage.passerelle.lock();
        let Some(passerelle) = passerelle else {
            continue;
        };

        // Un échec de la sonde elle-même (session ICMP impossible à créer) compte
        // comme des paquets tous perdus : de l'extérieur, le résultat est le même.
        let (envoyes, recus) = match EspPing::new(0).ping(passerelle, &conf) {
            Ok(resume) => (resume.transmitted, resume.received),
            Err(e) => {
                warn!("Sonde réseau : échec du sondage de {passerelle} : {e:?}");
                (PAQUETS_PAR_SONDE, 0)
            }
        };

        if recus > 0 {
            *partage.dernier_succes.lock() = Some(Instant::now());
        }

        {
            let mut fenetre = partage.fenetre.lock();
            if fenetre.len() == TAILLE_FENETRE {
                fenetre.pop_front();
            }
            fenetre.push_back((envoyes, recus));
        }

        if recus < envoyes {
            warn!(
                "Sonde réseau : {passerelle} — {recus}/{envoyes} paquet(s) reçu(s) sur ce sondage"
            );
        }
    }
}
