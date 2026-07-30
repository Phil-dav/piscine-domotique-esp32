"""Moniteur série sans reset pour l'ESP32 (projet piscine, c:\\rust\\3).

Contrairement à `espflash monitor --no-reset`, ce script fixe DTR et RTS à
`False` AVANT d'ouvrir le port : aucune impulsion n'est donc envoyée sur ces
lignes a la connexion, ce qui evite de declencher le circuit auto-reset de la
carte (condensateurs couples a EN/GPIO0) et de la coincer dans le bootloader
ROM (voir memoire "piege moniteur serie bootloader" - incident du 28/07/2026).

Utilisation : python moniteur_serie.py [PORT] [VITESSE]
Par defaut : port COM4, vitesse 115200 (comme espflash).
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
DOSSIER_LOGS = Path(r"C:\rust\3\logs")


def main() -> None:
    port = sys.argv[1] if len(sys.argv) > 1 else PORT_PAR_DEFAUT
    vitesse = int(sys.argv[2]) if len(sys.argv) > 2 else VITESSE_PAR_DEFAUT

    reponse = input("Enregistrer aussi dans un fichier ? (o/N) : ").strip().lower()
    fichier_fonctionnement = None
    fichier_defauts = None
    if reponse in ("o", "oui", "y", "yes"):
        DOSSIER_LOGS.mkdir(parents=True, exist_ok=True)
        base = datetime.now().strftime("log_%Y-%m-%d_%Hh%M")
        chemin_fonctionnement = DOSSIER_LOGS / f"{base}_fonctionnement.txt"
        chemin_defauts = DOSSIER_LOGS / f"{base}_defauts.txt"
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
