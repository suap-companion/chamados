#!/bin/sh
# Cria a chave que assina o APK do app (uma vez) e mostra como cadastrá-la nos segredos do GitHub.
#
# A chave é SUA: guarde o arquivo e as senhas em lugar seguro (cofre de senhas). Quem perde a chave
# não consegue mais publicar atualizações que instalem por cima do app já instalado. Nada disto vai
# para o repositório (`*.jks` está no .gitignore).
set -eu
FILE="${1:-chamados-release.jks}"
ALIAS="${2:-chamados}"
[ ! -e "$FILE" ] || { echo "já existe $FILE: não vou sobrescrever"; exit 1; }
keytool -genkeypair -v -keystore "$FILE" -alias "$ALIAS" -keyalg RSA -keysize 4096 -validity 10000 \
  -dname "CN=Chamados, O=chamados"
cat <<EOF

Chave criada em $FILE (alias $ALIAS). Agora cadastre os segredos do repositório (cada comando pede o valor):

  base64 -w0 "$FILE" | gh secret set ANDROID_KEYSTORE_B64 --repo suap-companion/chamados
  gh secret set ANDROID_KEYSTORE_PASSWORD --repo suap-companion/chamados
  gh secret set ANDROID_KEY_ALIAS --repo suap-companion/chamados --body "$ALIAS"
  gh secret set ANDROID_KEY_PASSWORD --repo suap-companion/chamados

Depois disso, cada release publicada anexa chamados-<tag>-android.apk.
EOF
