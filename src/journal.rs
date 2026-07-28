//! Journaux persistés (sessions pompe, bilans journaliers, alertes), pour la page
//! "Journaux" du dashboard (portage du `LogManager` du projet C++ de référence).
//!
//! Le C++ écrit des CSV mensuels sur LittleFS. Faute d'équivalent filesystem simple
//! côté esp-idf-hal/Rust, on stocke à la place un historique borné (dernières N lignes)
//! par catégorie dans la NVS, sous forme de texte CSV — persistant entre redémarrages,
//! mais sans le découpage mensuel ni la purge par ancienneté du C++.

use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use log::warn;

const ESPACE_NOM: &str = "journal";

const CLE_SESSIONS: &str = "sessions";
const CLE_BILANS: &str = "bilans";
const CLE_ALERTES: &str = "alertes";
const CLE_TIMELINE: &str = "timeline";
const CLE_TIMELINE_DATE: &str = "timeline_date";
const CLE_POMPE_JOUR: &str = "pompe_jour";
const CLE_POMPE_JOUR_DATE: &str = "pompe_jour_date";

const MAX_LIGNES_SESSIONS: usize = 25;
const MAX_LIGNES_BILANS: usize = 31;
const MAX_LIGNES_ALERTES: usize = 25;

const ENTETE_SESSIONS: &str = "date,debut,fin,duree_min,T_eau,mode,pompe,cause_fin";
const ENTETE_BILANS: &str = "date,objectif_h,fait_h,nb_sessions,nb_alertes,mode_jour";
const ENTETE_ALERTES: &str = "date,heure,type,valeur";

/// Une session pompe terminée, prête à être ajoutée au journal.
pub struct SessionTerminee<'a> {
    pub date: &'a str,
    pub debut: &'a str,
    pub fin: &'a str,
    pub duree_min: i64,
    pub t_eau: f32,
    pub mode: &'a str,
    pub cause_fin: &'a str,
    pub pompe_on: bool,
}

pub struct Journal {
    nvs: EspNvs<NvsDefault>,
}

impl Journal {
    pub fn ouvrir(partition: EspDefaultNvsPartition) -> anyhow::Result<Self> {
        let nvs = EspNvs::new(partition, ESPACE_NOM, true)
            .map_err(|e| anyhow::anyhow!("Erreur ouverture NVS ({}) : {:?}", ESPACE_NOM, e))?;
        Ok(Journal { nvs })
    }

    fn lire_blob(&self, cle: &str) -> String {
        let taille = match self.nvs.blob_len(cle) {
            Ok(Some(t)) if t > 0 => t,
            _ => return String::new(),
        };
        let mut buf = vec![0u8; taille];
        match self.nvs.get_blob(cle, &mut buf) {
            Ok(Some(donnees)) => String::from_utf8_lossy(donnees).into_owned(),
            _ => String::new(),
        }
    }

    /// Ajoute une ligne CSV, en ne conservant que les `max_lignes` plus récentes.
    fn ajouter_ligne(&mut self, cle: &str, ligne: &str, max_lignes: usize) {
        let mut contenu = self.lire_blob(cle);
        contenu.push_str(ligne);
        contenu.push('\n');

        let lignes: Vec<&str> = contenu.lines().collect();
        let debut = lignes.len().saturating_sub(max_lignes);
        let recadre = lignes[debut..].join("\n") + "\n";

        if let Err(e) = self.nvs.set_blob(cle, recadre.as_bytes()) {
            warn!("Journal : échec écriture NVS '{}' : {:?}", cle, e);
        }
    }

    pub fn enregistrer_session(&mut self, session: SessionTerminee) {
        let ligne = format!(
            "{},{},{},{},{:.1},{},{},{}",
            session.date,
            session.debut,
            session.fin,
            session.duree_min,
            session.t_eau,
            session.mode,
            if session.pompe_on { "ON" } else { "OFF" },
            session.cause_fin,
        );
        self.ajouter_ligne(CLE_SESSIONS, &ligne, MAX_LIGNES_SESSIONS);
    }

    pub fn enregistrer_bilan(
        &mut self,
        date: &str,
        objectif_h: f32,
        fait_h: f32,
        nb_sessions: u32,
        nb_alertes: u32,
        mode_jour: &str,
    ) {
        let ligne =
            format!("{date},{objectif_h:.1},{fait_h:.2},{nb_sessions},{nb_alertes},{mode_jour}");
        self.ajouter_ligne(CLE_BILANS, &ligne, MAX_LIGNES_BILANS);
    }

    pub fn enregistrer_alerte(&mut self, date: &str, heure: &str, type_alerte: &str, valeur: &str) {
        let ligne = format!("{date},{heure},{type_alerte},{valeur}");
        self.ajouter_ligne(CLE_ALERTES, &ligne, MAX_LIGNES_ALERTES);
    }

    pub fn lire_sessions(&self) -> String {
        format!("{ENTETE_SESSIONS}\n{}", self.lire_blob(CLE_SESSIONS))
    }

    pub fn lire_bilans(&self) -> String {
        format!("{ENTETE_BILANS}\n{}", self.lire_blob(CLE_BILANS))
    }

    pub fn lire_alertes(&self) -> String {
        format!("{ENTETE_ALERTES}\n{}", self.lire_blob(CLE_ALERTES))
    }

    /// Sauvegarde l'intégralité des segments de la timeline du jour (barre "Phase
    /// d'optimisation active" du dashboard), pour qu'elle survive à un redémarrage.
    /// Contrairement à `ajouter_ligne` (historique cumulatif borné), c'est un
    /// instantané complet qui écrase le précédent à chaque appel : la timeline
    /// représente l'état du jour en cours, pas un historique à conserver dans le
    /// temps. La date est sauvegardée à part, pour savoir au redémarrage si ces
    /// segments appartiennent encore au jour en cours (voir `charger_timeline`).
    pub fn sauvegarder_timeline(&mut self, date: &str, segments: &[crate::historique_modes::Segment]) {
        let mut contenu = String::new();
        for s in segments {
            contenu.push_str(&format!("{:.3},{:.3},{}\n", s.debut, s.fin, s.type_segment));
        }
        if let Err(e) = self.nvs.set_blob(CLE_TIMELINE, contenu.as_bytes()) {
            warn!("Journal : échec écriture NVS '{}' : {:?}", CLE_TIMELINE, e);
        }
        if let Err(e) = self.nvs.set_blob(CLE_TIMELINE_DATE, date.as_bytes()) {
            warn!("Journal : échec écriture NVS '{}' : {:?}", CLE_TIMELINE_DATE, e);
        }
    }

    /// Recharge les segments sauvegardés, uniquement si leur date correspond à
    /// `date_actuelle` — sinon ils appartiennent à un jour précédent (redémarrage
    /// après minuit) et doivent être ignorés, comme un changement de jour normal.
    pub fn charger_timeline(&self, date_actuelle: &str) -> Vec<crate::historique_modes::Segment> {
        if self.lire_blob(CLE_TIMELINE_DATE) != date_actuelle {
            return Vec::new();
        }
        self.lire_blob(CLE_TIMELINE)
            .lines()
            .filter_map(|ligne| {
                let mut champs = ligne.split(',');
                let debut = champs.next()?.parse().ok()?;
                let fin = champs.next()?.parse().ok()?;
                let type_segment = champs.next()?.parse().ok()?;
                Some(crate::historique_modes::Segment { debut, fin, type_segment })
            })
            .collect()
    }

    /// Sauvegarde le temps de marche et le nombre de sessions cumulés du jour
    /// en cours (voir `charger_pompe_jour`) — sinon ces compteurs, purement en
    /// RAM (`pompe.rs`), repartaient de zéro à chaque redémarrage, contrairement
    /// à la timeline des modes (déjà persistée) à laquelle ils sont pourtant
    /// affichés côte à côte sur le dashboard.
    pub fn sauvegarder_pompe_jour(&mut self, date: &str, heures: f32, sessions: u32) {
        let contenu = format!("{heures:.4},{sessions}");
        if let Err(e) = self.nvs.set_blob(CLE_POMPE_JOUR, contenu.as_bytes()) {
            warn!("Journal : échec écriture NVS '{}' : {:?}", CLE_POMPE_JOUR, e);
        }
        if let Err(e) = self.nvs.set_blob(CLE_POMPE_JOUR_DATE, date.as_bytes()) {
            warn!("Journal : échec écriture NVS '{}' : {:?}", CLE_POMPE_JOUR_DATE, e);
        }
    }

    /// Recharge (heures, sessions) du jour, uniquement si la date sauvegardée
    /// correspond à `date_actuelle` — même logique que `charger_timeline`.
    pub fn charger_pompe_jour(&self, date_actuelle: &str) -> Option<(f32, u32)> {
        if self.lire_blob(CLE_POMPE_JOUR_DATE) != date_actuelle {
            return None;
        }
        let contenu = self.lire_blob(CLE_POMPE_JOUR);
        let mut champs = contenu.trim().split(',');
        let heures = champs.next()?.parse().ok()?;
        let sessions = champs.next()?.parse().ok()?;
        Some((heures, sessions))
    }

    pub fn effacer_tout(&mut self) {
        for cle in [
            CLE_SESSIONS,
            CLE_BILANS,
            CLE_ALERTES,
            CLE_TIMELINE,
            CLE_TIMELINE_DATE,
            CLE_POMPE_JOUR,
            CLE_POMPE_JOUR_DATE,
        ] {
            if let Err(e) = self.nvs.remove(cle) {
                warn!("Journal : échec effacement NVS '{}' : {:?}", cle, e);
            }
        }
    }
}
