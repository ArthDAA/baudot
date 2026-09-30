#!/bin/sh
# Compile le firmware Var dans le conteneur QMK officiel (ghcr.io/qmk/qmk_cli).
#
# Depuis la racine du dépôt, sous PowerShell :
#   docker run --rm -v omnikey-qmk:/qmk_firmware `
#     -v "${PWD}\firmware\qmk-var:/omnikey:ro" `
#     -v "${PWD}\firmware\qmk-var\build:/out" `
#     ghcr.io/qmk/qmk_cli sh /omnikey/build.sh ansi rgb
#
# Arguments : disposition (ansi | iso), variante (rgb | white).
# Le fork Keychron est gardé dans le volume "omnikey-qmk" entre deux
# compilations ; le binaire arrive dans firmware/qmk-var/build/.
set -eu

LAYOUT=${1:-ansi}
VARIANT=${2:-rgb}
QMK=/qmk_firmware

if [ ! -d "$QMK/.git" ]; then
    git clone --depth 1 --recurse-submodules --shallow-submodules \
        -b wireless_playground https://github.com/Keychron/qmk_firmware.git "$QMK"
fi
cd "$QMK"

# Dépendances Python propres au fork (l'image ne garde rien d'un lancement à
# l'autre, d'où l'installation à chaque compilation).
QMK_PY=/opt/uv/tools/qmk/bin/python3
[ -x "$QMK_PY" ] || QMK_PY=python3
"$QMK_PY" -m pip install -q --disable-pip-version-check -r requirements.txt

# Repart toujours du fichier d'origine avant d'appliquer le patch.
git checkout -- keyboards/keychron/common/keychron_raw_hid.c
patch -p1 < /omnikey/keychron_raw_hid.patch

KM="keyboards/keychron/k3_max/$LAYOUT/$VARIANT/keymaps/var"
rm -rf "$KM"
mkdir -p "$KM"
cp /omnikey/src/* /omnikey/keymaps/"$LAYOUT"/* "$KM"/

make "keychron/k3_max/$LAYOUT/$VARIANT:var"
cp ".build/keychron_k3_max_${LAYOUT}_${VARIANT}_var.bin" /out/
echo "Firmware : firmware/qmk-var/build/keychron_k3_max_${LAYOUT}_${VARIANT}_var.bin"
