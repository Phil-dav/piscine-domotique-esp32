use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, NaiveDateTime, Weekday};

/// Renvoie le dernier dimanche du mois donné (mars ou octobre ont tous deux 31 jours).
fn dernier_dimanche(annee: i32, mois: u32) -> NaiveDate {
    let mut jour = NaiveDate::from_ymd_opt(annee, mois, 31).unwrap();
    while jour.weekday() != Weekday::Sun {
        jour = jour.pred_opt().unwrap();
    }
    jour
}

/// Règle européenne : heure d'été du dernier dimanche de mars 01h00 UTC
/// au dernier dimanche d'octobre 01h00 UTC.
fn heure_ete_active(utc: NaiveDateTime) -> bool {
    let annee = utc.year();
    let debut = dernier_dimanche(annee, 3).and_hms_opt(1, 0, 0).unwrap();
    let fin = dernier_dimanche(annee, 10).and_hms_opt(1, 0, 0).unwrap();
    utc >= debut && utc < fin
}

/// Convertit une date/heure UTC en heure locale Europe/Paris (CET/CEST).
pub fn vers_heure_paris(utc: NaiveDateTime) -> NaiveDateTime {
    let decalage = if heure_ete_active(utc) { 2 } else { 1 };
    utc + ChronoDuration::hours(decalage)
}
