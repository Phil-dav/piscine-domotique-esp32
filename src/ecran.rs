use core::fmt::Write;
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};
use heapless::String;

/// Affiche 4 lignes de texte aux positions verticales standard de l'écran (128x64).
fn quatre_lignes<D>(display: &mut D, l1: &str, l2: &str, l3: &str, l4: &str) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    display
        .clear(BinaryColor::Off)
        .map_err(|e| anyhow::anyhow!("Erreur clear : {:?}", e))?;
    Text::new(l1, Point::new(0, 8), style)
        .draw(display)
        .map_err(|e| anyhow::anyhow!("Erreur dessin : {:?}", e))?;
    Text::new(l2, Point::new(0, 24), style)
        .draw(display)
        .map_err(|e| anyhow::anyhow!("Erreur dessin : {:?}", e))?;
    Text::new(l3, Point::new(0, 40), style)
        .draw(display)
        .map_err(|e| anyhow::anyhow!("Erreur dessin : {:?}", e))?;
    Text::new(l4, Point::new(0, 56), style)
        .draw(display)
        .map_err(|e| anyhow::anyhow!("Erreur dessin : {:?}", e))?;

    Ok(())
}

/// Formate un nombre d'heures en "XhYY" (ex: 2.25 -> "2h15").
fn formater_hm(heures: f32) -> String<8> {
    let total_min = (heures * 60.0 + 0.5) as i32;
    let mut s: String<8> = String::new();
    write!(s, "{}h{:02}", total_min / 60, total_min % 60).ok();
    s
}

/// Formate une durée en secondes en "XjYhZm" ou "XhYYm".
fn formater_duree(secondes: u64) -> String<16> {
    let jours = secondes / 86400;
    let heures = (secondes % 86400) / 3600;
    let minutes = (secondes % 3600) / 60;
    let mut s: String<16> = String::new();
    if jours > 0 {
        write!(s, "{}j{}h{}m", jours, heures, minutes).ok();
    } else {
        write!(s, "{}h{:02}m", heures, minutes).ok();
    }
    s
}

/// Affiche l'écran de recherche de connexion Wi-Fi (au démarrage).
pub fn afficher_connexion<D>(display: &mut D) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    quatre_lignes(display, "Piscine ESP32", "Wi-Fi :", "Recherche...", "")
}

/// Page 0 (par défaut) : état pompe, température air/eau, humidité.
pub fn afficher_capteurs<D>(
    display: &mut D,
    pompe_active: bool,
    temperature_air: f32,
    humidite: f32,
    temperature_eau: f32,
) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    let mut ligne_pompe: String<32> = String::new();
    write!(
        ligne_pompe,
        "Pompe: {}",
        if pompe_active { "MARCHE" } else { "ARRET" }
    )
    .ok();

    let mut ligne_air: String<32> = String::new();
    write!(ligne_air, "Air:{:.1}C Hum:{:.0}%", temperature_air, humidite).ok();

    let mut ligne_eau: String<32> = String::new();
    write!(ligne_eau, "Eau: {:.1} C", temperature_eau).ok();

    quatre_lignes(display, "Piscine ESP32", &ligne_pompe, &ligne_air, &ligne_eau)
}

/// Page 1 : état Wi-Fi connecté, adresse IP et niveau de réception.
pub fn afficher_connecte<D>(display: &mut D, ip: &str, rssi_dbm: Option<i32>) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    let mut ligne_ip: String<32> = String::new();
    write!(ligne_ip, "IP : {}", ip).ok();

    let mut ligne_rssi: String<32> = String::new();
    match rssi_dbm {
        Some(v) => write!(ligne_rssi, "Signal : {} dBm", v).ok(),
        None => write!(ligne_rssi, "Signal : --").ok(),
    };

    quatre_lignes(display, "Piscine ESP32", "Wifi connecte", &ligne_ip, &ligne_rssi)
}

/// Page 2 : pompe & filtration — état/mode, heures faites/objectif, régime ou plage, sessions du jour.
#[allow(clippy::too_many_arguments)]
pub fn afficher_pompe<D>(
    display: &mut D,
    pompe_active: bool,
    boost_actif: bool,
    mode_texte: &str,
    heures_faites: f32,
    heures_cibles: f32,
    anti_gel: bool,
    canicule: bool,
    plage_debut: f32,
    plage_fin: f32,
    sessions_jour: u32,
) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    let mut l1: String<32> = String::new();
    write!(
        l1,
        "Pompe:{} [{}]",
        if pompe_active { "ON" } else { "OFF" },
        if boost_actif { "BOOST" } else { mode_texte }
    )
    .ok();

    let mut l2: String<32> = String::new();
    write!(
        l2,
        "Fait:{} / {}",
        formater_hm(heures_faites),
        formater_hm(heures_cibles)
    )
    .ok();

    let mut l3: String<32> = String::new();
    if anti_gel {
        write!(l3, "Mode: ANTI-GEL").ok();
    } else if canicule {
        write!(l3, "Mode: CANICULE").ok();
    } else {
        write!(l3, "Plage:{:.0}h-{:.0}h", plage_debut, plage_fin).ok();
    };

    let mut l4: String<32> = String::new();
    write!(l4, "Sessions: {}", sessions_jour).ok();

    quatre_lignes(display, &l1, &l2, &l3, &l4)
}

/// Page 3 : sécurités & fonctionnement — niveau d'eau, défaut moteur, sécurité globale, uptime.
pub fn afficher_securite<D>(
    display: &mut D,
    niveau_ok: bool,
    defaut_moteur_actif: bool,
    defaut_moteur_verrouille: bool,
    systeme_sur: bool,
    uptime_secondes: u64,
) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    let mut l1: String<32> = String::new();
    write!(l1, "Niveau: {}", if niveau_ok { "OK" } else { "MANQUE !" }).ok();

    let mut l2: String<32> = String::new();
    write!(
        l2,
        "Moteur: {}",
        if !defaut_moteur_actif {
            "OK"
        } else if defaut_moteur_verrouille {
            "VERROU !"
        } else {
            "DEFAUT !"
        }
    )
    .ok();

    let mut l3: String<32> = String::new();
    write!(l3, "Secure: {}", if systeme_sur { "OK" } else { "ALERTE !" }).ok();

    let mut l4: String<32> = String::new();
    write!(l4, "Uptime:{}", formater_duree(uptime_secondes)).ok();

    quatre_lignes(display, &l1, &l2, &l3, &l4)
}

/// Page 4 : bilan journalier — date, filtration faite/objectif, température eau min/max du jour.
pub fn afficher_bilan<D>(
    display: &mut D,
    date_texte: &str,
    heures_faites: f32,
    heures_cibles: f32,
    temp_min: Option<f32>,
    temp_max: Option<f32>,
) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    let mut l1: String<32> = String::new();
    write!(l1, "Bilan {}", date_texte).ok();

    let mut l2: String<32> = String::new();
    write!(
        l2,
        "Filtr:{} / {}",
        formater_hm(heures_faites),
        formater_hm(heures_cibles)
    )
    .ok();

    let mut l3: String<32> = String::new();
    match temp_min {
        Some(t) => write!(l3, "T.min:{:.1}C", t).ok(),
        None => write!(l3, "T.min: --").ok(),
    };

    let mut l4: String<32> = String::new();
    match temp_max {
        Some(t) => write!(l4, "T.max:{:.1}C", t).ok(),
        None => write!(l4, "T.max: --").ok(),
    };

    quatre_lignes(display, &l1, &l2, &l3, &l4)
}

/// Écran d'alerte clignotant, s'impose sur toutes les pages en cas de problème réel.
/// `cause1`/`cause2` vides = pas de ligne affichée pour cette cause.
pub fn afficher_alerte<D>(display: &mut D, cause1: &str, cause2: &str) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);

    display
        .clear(BinaryColor::Off)
        .map_err(|e| anyhow::anyhow!("Erreur clear : {:?}", e))?;
    Text::new("** ALERTE **", Point::new(16, 8), style)
        .draw(display)
        .map_err(|e| anyhow::anyhow!("Erreur dessin : {:?}", e))?;
    if !cause1.is_empty() {
        Text::new(cause1, Point::new(0, 32), style)
            .draw(display)
            .map_err(|e| anyhow::anyhow!("Erreur dessin : {:?}", e))?;
    }
    if !cause2.is_empty() {
        Text::new(cause2, Point::new(0, 48), style)
            .draw(display)
            .map_err(|e| anyhow::anyhow!("Erreur dessin : {:?}", e))?;
    }

    Ok(())
}

/// Efface l'écran (utilisé pour le clignotement de l'alerte).
pub fn effacer<D>(display: &mut D) -> anyhow::Result<()>
where
    D: DrawTarget<Color = BinaryColor>,
    D::Error: core::fmt::Debug,
{
    display
        .clear(BinaryColor::Off)
        .map_err(|e| anyhow::anyhow!("Erreur clear : {:?}", e))
}
