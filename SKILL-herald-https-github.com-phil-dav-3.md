---
name: corriger-problemes-herald
description: Corrige les 110 problème(s) détecté(s) par Herald dans https://github.com/Phil-dav/3 (score 65/100, grade C). À utiliser pour résoudre, règle par règle, les relevés de sécurité, qualité, performance et fiabilité.
---

# Corriger les problèmes relevés par Herald

**Cible :** `https://github.com/Phil-dav/3`  
**État :** score 65/100 (grade C), 110 problème(s) : 19 critique(s), 62 à revoir, 29 info(s).

## Objectif

Corriger les problèmes ci-dessous pour remonter le score, en traitant **une règle à la fois** et en commençant par les plus graves.

## Méthode

1. Prends les règles dans l'ordre (les critiques d'abord).
2. Pour chaque règle, ouvre **chaque fichier listé** et applique la correction indiquée.
3. Reste minimal : ne change que le nécessaire, en gardant le style du code environnant.
4. Après chaque lot, relance build / lint / tests pour vérifier la non-régression.
5. N'introduis ni secret, ni code mort, ni dépendance obsolète.

## Problèmes à corriger

### 1. [CRITIQUE] `no-unwrap-in-prod`, 17 occurrence(s)

Panic possible : .unwrap()/.expect() fait paniquer la tâche sur une erreur (→ 500 ou worker qui tombe en prod) ; propage avec ?, gère par match, ou utilise unwrap_or/ok_or avec un message explicite

**Fichiers à corriger :**

- `src/main.rs:157`
- `src/main.rs:246`
- `src/main.rs:251`
- `src/main.rs:334`
- `src/main.rs:424`
- `src/main.rs:585`
- `src/temps.rs:5`
- `src/temps.rs:7`
- `src/temps.rs:16`
- `src/temps.rs:17`
- `src/web_server.rs:54`
- `src/web_server.rs:133`
- `src/web_server.rs:154`
- `src/web_server.rs:195`
- `src/web_server.rs:236`
- `src/web_server.rs:250`
- `src/web_server.rs:287`

### 2. [CRITIQUE] `no_inner_html`, 2 occurrence(s)

Avoid setting innerHTML/outerHTML directly: major XSS risk

**Fichiers à corriger :**

- `src/script.js:25`
- `src/script.js:609`

### 3. [À REVOIR] `ai_duplicate_block`, 11 occurrence(s)

Bloc de 6 lignes dupliqué (déjà présent ligne 130) — copier-coller massif

**Fichiers à corriger :**

- `src/ecran.rs:176`
- `src/ecran.rs:215`
- `src/ecran.rs:227`
- `src/ecran.rs:250`
- `src/index.html:1131`
- `src/web_server.rs:179`
- `src/web_server.rs:215`
- `src/web_server.rs:237`
- `src/web_server.rs:255`
- `src/web_server.rs:288`
- `src/wifi.rs:47`

### 4. [À REVOIR] `loose-equality`, 11 occurrence(s)

Égalité faible (== / !=) — effectue une coercition de type source de bugs subtils ; utilise === / !==

**Fichiers à corriger :**

- `src/script.js:59`
- `src/script.js:60`
- `src/script.js:64`
- `src/script.js:187`
- `src/script.js:371`
- `src/script.js:404`
- `src/script.js:463`
- `src/script.js:464`
- `src/script.js:465`
- `src/script.js:466`
- `src/script.js:716`

### 5. [À REVOIR] `long-line`, 7 occurrence(s)

Line too long: 122 chars (max 120)

**Fichiers à corriger :**

- `src/gps.rs:17`
- `src/gps.rs:58`
- `src/index.html:1127`
- `src/index.html:1192`
- `src/main.rs:620`
- `src/script.js:458`
- `src/web_server.rs:91`

### 6. [À REVOIR] `function-too-long`, 7 occurrence(s)

Function is too long: 473 logical lines (max 60) — split it into smaller units

**Fichiers à corriger :**

- `src/main.rs:40`
- `src/main.rs:470`
- `src/script.js:38`
- `src/script.js:47`
- `src/script.js:462`
- `src/web_server.rs:22`
- `src/web_server.rs:53`

### 7. [À REVOIR] `high-complexity`, 5 occurrence(s)

High cyclomatic complexity: 17 (max 15) — simplify branching or extract helpers

**Fichiers à corriger :**

- `src/filtration_auto.rs:31`
- `src/main.rs:40`
- `src/main.rs:470`
- `src/script.js:47`
- `src/script.js:462`

### 8. [À REVOIR] `no_alert`, 4 occurrence(s)

Avoid alert/confirm/prompt: blocking dialogs are bad UX and often debug leftovers

**Fichiers à corriger :**

- `src/script.js:323`
- `src/script.js:333`
- `src/script.js:343`
- `src/script.js:357`

### 9. [À REVOIR] `swallowed-error`, 4 occurrence(s)

Erreur avalée silencieusement (except: pass / if err != nil {} vide / unwrap_or_default) — l'échec disparaît sans trace ni traitement ; logge-le, propage-le, ou gère-le explicitement

**Fichiers à corriger :**

- `src/pompe.rs:57`
- `src/web_server.rs:50`
- `src/web_server.rs:74`
- `src/web_server.rs:77`

### 10. [À REVOIR] `too-many-params`, 3 occurrence(s)

Too many parameters: 11 (max 5) — group related arguments into an object/struct

**Fichiers à corriger :**

- `src/ecran.rs:118`
- `src/ecran.rs:169`
- `src/ecran.rs:208`

### 11. [À REVOIR] `max_file_lines`, 3 occurrence(s)

File too long: 1220 lines (max 500)

**Fichiers à corriger :**

- `src/index.html:1`
- `src/main.rs:1`
- `src/script.js:1`

### 12. [À REVOIR] `no_unused_vars`, 3 occurrence(s)

Unused variable 'clockInterval' — remove it or prefix with '_'

**Fichiers à corriger :**

- `src/script.js:797`
- `src/script.js:798`
- `src/web_server.rs:78`

### 13. [À REVOIR] `no_debug_print`, 2 occurrence(s)

Avoid leaving debug print statements in production code

**Fichiers à corriger :**

- `src/script.js:320`
- `src/script.js:348`

### 14. [À REVOIR] `deep-nesting`, 1 occurrence(s)

Nesting too deep: 5 levels (max 4) — extract helpers or use early returns / guard clauses

**Fichiers à corriger :**

- `src/main.rs:470`

### 15. [À REVOIR] `redundant-boolean`, 1 occurrence(s)

Redundant boolean literal — use the condition directly (`if (x)`, `return cond`) instead of comparing to or returning true/false

**Fichiers à corriger :**

- `src/script.js:372`

### 16. [INFO] `ai_unused_generated`, 15 occurrence(s)

Fonction 'traiter_trame' jamais appelée dans ce fichier — possible code mort généré

**Fichiers à corriger :**

- `src/gps.rs:54`
- `src/pcf8574.rs:33`
- `src/pompe.rs:116`
- `src/script.js:273`
- `src/script.js:287`
- `src/script.js:316`
- `src/script.js:327`
- `src/script.js:337`
- `src/script.js:347`
- `src/script.js:670`
- `src/script.js:680`
- `src/script.js:729`
- `src/script.js:735`
- `src/script.js:754`
- `src/script.js:762`

### 17. [INFO] `ai-placeholder-url`, 9 occurrence(s)

Valeur factice générée (example.com, your-domain, INSERT_..._HERE…) — remplace par la vraie

**Fichiers à corriger :**

- `src/main.rs:200`
- `src/main.rs:290`
- `src/main.rs:296`
- `src/main.rs:303`
- `src/pompe.rs:20`
- `src/pompe.rs:36`
- `src/pompe.rs:77`
- `src/pompe.rs:112`
- `src/pompe.rs:119`

### 18. [INFO] `mvc`, 3 occurrence(s)

Mélange des couches MVC — persistance ou présentation dans la logique applicative ; isole l'accès aux données (modèle) et le rendu (vue) hors du contrôleur

**Fichiers à corriger :**

- `src/script.js:25`
- `src/script.js:596`
- `src/script.js:609`

### 19. [INFO] `outdated-dependency`, 1 occurrence(s)

heapless (0.9.1) obsolète · maj: 0.9.3 · doc: https://crates.io/crates/heapless

**Fichiers à corriger :**

- `Cargo.toml`

### 20. [INFO] `vulnerable-dependency`, 1 occurrence(s)

chrono (0.4): Potential segfault in `localtime_r` invocations [RUSTSEC-2020-0159] · maj: 0.4.45 · doc: https://osv.dev/vulnerability/RUSTSEC-2020-0159

**Fichiers à corriger :**

- `Cargo.toml`

---

_Skill généré par Async Herald · herald.codes · sans IA, aucun code stocké._