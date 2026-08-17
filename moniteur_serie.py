"""Moniteur série sans reset pour l'ESP32 (projet piscine, c:\\rust\\3).

Contrairement à `espflash monitor --no-reset`, ce script fixe DTR et RTS à
`False` AVANT d'ouvrir le port : aucune impulsion n'est donc envoyée sur ces
lignes a la connexion, ce qui evite de declencher le circuit auto-reset de la
carte (condensateurs couples a EN/GPIO0) et de la coincer dans le bootloader
ROM (voir memoire "piege moniteur serie bootloader" - incident du 28/07/2026).

Utilisation : python moniteur_serie.py [PORT] [VITESSE] [DOSSIER_LOGS] [ETIQUETTE]
Par defaut : port COM4, vitesse 115200 (comme espflash), logs dans C:\\rust\\3\\logs.

Le troisieme argument permet de capturer une autre carte sans melanger les
journaux : depuis le 16/08/2026 une carte espion Wi-Fi (ESP32 sur COM7) tourne
en parallele de l'automate piscine, et il faut pouvoir comparer les deux points de
vue au meme instant. Voir l'alias PowerShell `connect` et sa cible `espion`.

Le quatrieme argument nomme la carte dans le fichier lui-meme :

    log_espion_2026-08-16_15h33_fonctionnement.txt

Les deux captures demarrant a une minute d'intervalle, leurs noms etaient quasi
identiques et seul le dossier les distinguait. Or des qu'un fichier est copie ou
ouvert dans un onglet d'editeur, ce contexte disparait. L'etiquette est placee juste
apres "log_" : comme tous les fichiers d'un meme dossier la partagent, le tri par
nom reste chronologique.

Ctrl+C pour quitter.
"""

import sys
from datetime import datetime
from pathlib import Path

import serial

PORT_PAR_DEFAUT = "COM4"
VITESSE_PAR_DEFAUT = 115200
# Dossier fixe, peu importe d'où `connect` est lancé (voir memoire "piege
# moniteur serie bootloader" et les echanges du 29/07/2026) - cree si absent.
DOSSIER_LOGS_PAR_DEFAUT = Path(r"C:\rust\3\logs")


def main() -> None:
    port = sys.argv[1] if len(sys.argv) > 1 else PORT_PAR_DEFAUT
    vitesse = int(sys.argv[2]) if len(sys.argv) > 2 else VITESSE_PAR_DEFAUT
    dossier_logs = Path(sys.argv[3]) if len(sys.argv) > 3 else DOSSIER_LOGS_PAR_DEFAUT
    etiquette = sys.argv[4] if len(sys.argv) > 4 else ""

    reponse = input("Enregistrer aussi dans un fichier ? (o/N) : ").strip().lower()
    fichier_fonctionnement = None
    fichier_defauts = None
    if reponse in ("o", "oui", "y", "yes"):
        dossier_logs.mkdir(parents=True, exist_ok=True)
        # Sans etiquette, on garde exactement l'ancien nommage : les captures ad hoc
        # (`connect COM5`) ne changent pas de forme.
        prefixe = "log_%s_" % etiquette if etiquette else "log_"
        base = datetime.now().strftime(prefixe + "%Y-%m-%d_%Hh%M")
        chemin_fonctionnement = dossier_logs / f"{base}_fonctionnement.txt"
        chemin_defauts = dossier_logs / f"{base}_defauts.txt"
        fichier_fonctionnement = open(chemin_fonctionnement, "w", encoding="utf-8")
        fichier_defauts = open(chemin_defauts, "w", encoding="utf-8")
        print(f"Enregistrement dans {chemin_fonctionnement} et {chemin_defauts}")

    ser = serial.Serial()
    ser.port = port
    ser.baudrate = vitesse
    ser.dtr = False
    ser.rts = False
    try:
        ser.open()
    except serial.SerialException as e:
        if "Accès refusé" in str(e) or "PermissionError" in str(e):
            print(f"{port} est déjà utilisé par une autre connexion (un autre terminal `connect` ouvert ?).")
        else:
            print(f"Impossible d'ouvrir {port} : {e}")
        sys.exit(1)

    print(f"Connecté sur {port} à {vitesse} bauds (DTR/RTS désactivés, pas de reset). Ctrl+C pour quitter.")

    try:
        while True:
            ligne = ser.readline()
            if ligne:
                texte = ligne.decode(errors="replace")
                print(texte, end="")
                # A l'ecran, tout reste mélangé comme avant. A l'enregistrement,
                # les lignes ESP-IDF commencant par W (avertissement) ou E (erreur)
                # partent dans le fichier "defauts", tout le reste (I = info,
                # bannieres de demarrage...) dans "fonctionnement".
                if texte[:1] in ("W", "E") and fichier_defauts:
                    fichier_defauts.write(texte)
                    fichier_defauts.flush()
                elif fichier_fonctionnement:
                    fichier_fonctionnement.write(texte)
                    fichier_fonctionnement.flush()
    except KeyboardInterrupt:
        print("\nArrêt du moniteur.")
    finally:
        ser.close()
        if fichier_fonctionnement:
            fichier_fonctionnement.close()
        if fichier_defauts:
            fichier_defauts.close()


if __name__ == "__main__":
    main()
