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

import serial

PORT_PAR_DEFAUT = "COM4"
VITESSE_PAR_DEFAUT = 115200


def main() -> None:
    port = sys.argv[1] if len(sys.argv) > 1 else PORT_PAR_DEFAUT
    vitesse = int(sys.argv[2]) if len(sys.argv) > 2 else VITESSE_PAR_DEFAUT

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
                print(ligne.decode(errors="replace"), end="")
    except KeyboardInterrupt:
        print("\nArrêt du moniteur.")
    finally:
        ser.close()


if __name__ == "__main__":
    main()
