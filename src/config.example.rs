// Version du firmware — repère simple pour confirmer qu'un téléversement a
// bien pris, affiché une fois au démarrage. Incrémentée automatiquement par
// Claude avant chaque flash (côté config.rs réel, pas ce gabarit).
pub const VERSION_FIRMWARE: &str = "1.0";

// Adresses I2C
pub const AHT10_ADDR: u8 = 0x38;
pub const PCF8574_ADDR: u8 = 0x21;
// L'écran OLED utilise 0x3C par défaut, géré automatiquement par la bibliothèque ssd1306

// Broches GPIO : câblage documenté directement en commentaire au point d'usage
// dans main.rs (l'API GPIO d'esp-idf-hal est typée à la compilation, un numéro
// de broche ne peut pas être injecté depuis une constante `u8`).
pub const GPS_BAUDRATE: u32 = 9600;

// PCF8574 (I2C 0x21) — broches P0-P7, logique inversée (LOW = actif).
// Mapping identique au projet C++ de référence, circuit physique inchangé.
// --- Sorties (P0-P3) ---
pub const PCF_RELAIS_POMPE: u8 = 0; // Pilotage pompe filtration (via In1 module relais)
                                    // Bit 1 : libre — réservé usage futur (alarme T° eau), non câblé pour l'instant.
                                    // Bit 2 : libre — ex-relais miroir pompe (feedback GPIO33),
                                    // retiré (jugé peu fiable), réservé usage futur.
pub const PCF_RELAIS_DEFAUT_SYSTEME: u8 = 3; // Signalisation défaut (niveau, moteur, sécurité)
                                             // --- Entrées (P4-P7) ---
pub const PCF_BOUTON_OLED: u8 = 4; // Bouton SW1 (écran)
pub const PCF_MODE_MANU: u8 = 5; // Interrupteur — position MANU
pub const PCF_MODE_AUTO: u8 = 6; // Interrupteur — position AUTO
pub const PCF_DEFAUT_RELAIS: u8 = 7; // Entrée J3 : défaut moteur / commande relais

// GPIO directs liés au même sous-système (pas sur le PCF8574) : GPIO33 est libre
// (ex-contact du relais miroir, retiré — réservé au futur capteur de courant pompe),
// GPIO34 = détecteur de niveau d'eau (ADC1_CH6) — câblage documenté au point d'usage
// dans main.rs.

// Wi-Fi — remplace par tes propres identifiants
pub const WIFI_SSID: &str = "TON_SSID_ICI";
pub const WIFI_PASSWORD: &str = "TON_MOT_DE_PASSE_ICI";

// Adafruit IO (cloud gratuit) — crée un compte sur io.adafruit.com. Feeds attendus :
// temp-eau, temp-air, humidite, pompe, mode, batterie, sortie-5v.
pub const ADAFRUIT_IO_USERNAME: &str = "TON_NOM_UTILISATEUR_ADAFRUIT_ICI";
pub const ADAFRUIT_IO_KEY: &str = "TA_CLE_ADAFRUIT_IO_ICI";

// Clé du "Group" Adafruit IO regroupant les feeds ci-dessus (menu Feeds > Groups sur
// io.adafruit.com), pour les envoyer en une seule requête HTTP au lieu de 7.
pub const ADAFRUIT_IO_GROUP_KEY: &str = "TA_CLE_DE_GROUPE_ICI";

/// Mettre à `false` pour couper tous les envois vers Adafruit IO.
pub const ADAFRUIT_IO_ACTIF: bool = true;

/// Mettre à `false` pour couper tous les logs sur le port série (info/avertissements).
/// Aucun impact sur le fonctionnement (Wi-Fi, pompe, etc.) : ça coupe uniquement le texte
/// affiché dans le moniteur série.
pub const LOGS_SERIE_ACTIFS: bool = true;

/// Mettre à `false` pour désactiver complètement le Wi-Fi (pas de connexion, pas de
/// serveur web, pas de synchro NTP).
pub const WIFI_ACTIF: bool = true;

/// Mettre à `true` pour servir le dashboard simplifié (température eau/air, humidité,
/// GPS/position/Wi-Fi, lien vers les graphiques Adafruit IO — rien d'autre : pas de
/// contrôle pompe, pas de mode, pas de journaux) au lieu du dashboard complet. Utile
/// pour un montage identique installé ailleurs, dédié à la simple consultation par un
/// tiers (ex. accès en lecture seule pour un proche). Aucune donnée capteur en moins
/// côté firmware, seule l'interface web change.
pub const DASHBOARD_SIMPLIFIE: bool = false;
