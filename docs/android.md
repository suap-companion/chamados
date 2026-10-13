# Aplicativo Android: núcleo Rust + UniFFI

Plano e decisões: issue #64. Este documento registra como compilar e testar.

## Pré-requisitos (uma vez)

- Android Studio com SDK (plataforma 37, build-tools, `cmdline-tools`), JDK 21 e Docker.
- NDK r29 **Linux** (usado dentro do Docker) e **Windows** (usado pelo Gradle):

  ```bash
  android --sdk=$HOME/.android-sdk-linux sdk --platform=linux_x86_64 install ndk/29.0.14206865
  android sdk install ndk/29.0.14206865
  ```
- Emulador: `system-images;android-36;google_apis;x86_64` e um AVD (`avdmanager create avd`).

## Compilar a biblioteca

```bash
scripts/build-android.sh      # .so das 3 ABIs + ligações Kotlin em android/app/src/main
cd android && ./gradlew assembleDebug
```

O perfil `mobile` (`Cargo.toml`) otimiza tamanho: `libchamados_mobile.so` com ~3,2 MB (arm64-v8a), ~2,0 MB (armeabi-v7a) e ~3,5 MB (x86_64).

## Rodar no emulador (spike M0)

```bash
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n br.edu.ifrn.chamados/.MainActivity
adb logcat -d -s CHAMADOS_SPIKE:I
```

O emulador enxerga o computador em `10.0.2.2` (o SUAP local, `http://10.0.2.2:8000`); o tráfego HTTP sem TLS só é liberado para esse endereço, em `network_security_config.xml`.

## Serviços compartilhados (`chamados-app`)

A crate `crates/chamados-app` guarda a lógica que o CLI e os aplicativos móveis usam em comum, sem argumentos nem saída de texto: download seguro de anexos, envio em lote de comentários/notas, filtro da listagem, uma rodada de `watch`, uma rodada de sincronização (com trava), `text_to_pdf` (texto vira PDF, pois o SUAP não aceita `.txt`) e `classify_shared` (separa o que o SUAP aceita do que recusa, com o motivo em português).

- Anexos: no máximo 10 MB (medido no SUAP; recusado antes do envio).
- Sessão: `SuapClient::import_cookie_header` recebe o cabeçalho `Cookie` da WebView de login; o app nunca vê a senha.

## Fachada `chamados-mobile` (M2)

O objeto `Chamados(dataDir, baseUrl)` é o que o aplicativo usa; nada nele é específico de Android.

| Função | O que faz |
|---|---|
| `importSessionCookies(header)` / `isAuthenticated()` / `logout()` | sessão a partir dos cookies da WebView de login (a senha nunca passa pelo app) |
| `list(queue, query)` / `show(id)` / `downloadAttachment(id, n, pasta)` | leitura: listas (suporte, meus, a fechar), chamado por inteiro e anexos |
| `applyShare(job)` | executa o que foi decidido na tela de compartilhamento: criar, anexar, comentar, nota interna, resolver, cancelar, reabrir, suspender |
| `classifyShared(arquivos, limite)` | separa o que o SUAP aceita (tipo e até 10 MB) do que recusa, com o motivo em português |
| `textToPdf(titulo, texto)` | texto como PDF, para anexar (o SUAP não aceita `.txt`) |

`applyShare` valida o pedido inteiro antes de enviar qualquer coisa (erro = nada foi enviado). Os anexos vão antes da mensagem e o primeiro passo que falhar interrompe a sequência; o resultado traz cada passo e `completed`, que o app devolve em `resumeFrom` para repetir só o que falta. Os arquivos chegam como caminhos já copiados para a pasta do aplicativo (nada de bytes atravessando a ligação).

## App v0: leitura (M3)

O app em `android/` (Jetpack Compose, Material 3) abre no login e depois na lista de chamados.

- **Login:** o SUAP é aberto num WebView (`accounts/login/`). Quando a navegação sai do login, o app lê os cookies do WebView, entrega a `Chamados.importSessionCookies` e confirma com `isAuthenticated`. A senha nunca passa pelo app, e o login com gov.br e 2FA continua funcionando.
- **Lista:** abas Meus, Suporte (com busca no texto) e A fechar.
- **Detalhe:** campos, linha do tempo e anexos; tocar num anexo baixa e abre no aplicativo escolhido (via `FileProvider`, pasta de cache).
- **Configurações:** endereço do SUAP (IFRN ou `http://10.0.2.2:8000/`, o SUAP local visto do emulador); cada endereço tem sessão própria. "Sair" esquece a sessão.
- **Sessão expirada:** uma falha de leitura com a sessão inválida volta ao login.

No emulador, se o teclado virtual não aparecer (o emulador usa o teclado do PC), ligue "Usar teclado na tela" em Configurações → Sistema → Teclado → Teclado físico.

Compilar e instalar: `sh scripts/build-android.sh`, depois `cd android && ./gradlew assembleDebug` e `adb install -r app/build/outputs/apk/debug/app-debug.apk`.

## Receber compartilhamentos (M4b)

O app aparece em **Compartilhar → Chamados** para texto e para os tipos que o SUAP aceita (pdf, jpg, png, doc/docx, xls/xlsx, csv), com um ou vários arquivos.

A `ShareActivity` copia os arquivos do `content://` para a pasta do app, classifica com `classifyShared` (os recusados aparecem riscados, com o motivo) e **sempre pergunta**: criar chamado, anexar, comentar, nota interna, resolver, cancelar, reabrir ou suspender. Ações que não cabem no que chegou ficam desabilitadas com a razão. O destino é escolhido numa lista (busca pelo número) ou digitando o número; para criar, informa-se o número do serviço.

Nada é enviado sem a revisão: o diálogo mostra a ação, o destino e os anexos (cancelar avisa que não pode ser desfeito). O resultado vem por passo; se algum falhar, "Tentar de novo só o que falhou" refaz apenas o restante (`resumeFrom`). As cópias temporárias são apagadas ao concluir ou ao fechar a tela. Sem sessão válida, a tela pede para entrar no app e continuar.

Testar no emulador (sem outro app): `adb shell "am start -n br.edu.ifrn.chamados/.ShareActivity -a android.intent.action.SEND -t text/plain --es android.intent.extra.TEXT 'texto'"`; para arquivos, indexe-os no MediaStore e passe `-d content://media/external/file/<id> --grant-read-uri-permission`.

Limites desta versão: o destino de "Criar" usa o campus e o centro padrão do SUAP; o compartilhamento pendente não sobrevive se o app for encerrado durante o login.

## Escrita no app (M4)

No detalhe do chamado, a seção **Ações** mostra o que faz sentido para a situação dele (o SUAP ainda decide pelas permissões, e a recusa dele aparece na tela):

- em andamento: Assumir, Iniciar atendimento, Comentar, Nota interna, Anexar arquivo, Resolver, Suspender, Cancelar chamado;
- resolvido: Reabrir, Fechar e avaliar (nota 1 a 5 e comentário), Comentar, Nota interna, Anexar arquivo;
- fechado ou cancelado: sem ações.

Cada ação abre uma confirmação (com o texto, os anexos opcionais e, no cancelamento, o aviso de que não pode ser desfeito). Ao dar certo, o detalhe é atualizado; ao falhar, o motivo do SUAP aparece, passo a passo.

**Novo chamado** (botão "Novo" na lista): número do serviço, interessado (vínculo; alguns serviços o exigem) e descrição, com até 3 anexos. Serviço e interessado ficam lembrados. O catálogo de serviços ainda não existe: é preciso saber o número.

A fachada ganhou `assume`, `startService` e `closeTicket(id, nota, comentário)`, e o `interested` entrou na criação de chamado (`ShareJob`).

## Gestão no app (M5)

Abaixo das ações, a seção **Gestão** do detalhe oferece o que o CLI já fazia: **atribuir** a outro atendente do grupo, **escalar** e **devolver** (com nota interna e, se quiser, quem recebe), **reclassificar** (serviço, campus e/ou centro, com justificativa), **adicionar/remover tags** e **incluir/remover interessados**.

Pessoas e tags são digitadas por nome ou matrícula (listas separadas por vírgula); o SUAP resolve o que bate. Se o texto for ambíguo ou não existir, o erro lista as opções disponíveis. Ao dar certo, o detalhe é atualizado e a mensagem diz quem foi atribuído, quais tags mudaram e quem entrou ou saiu.

A fachada ganhou `assign`, `escalate`, `returnTicket`, `reclassify`, `addTags`/`removeTags` e `addInterested`/`removeInterested`. Fica para depois escolher atendentes e tags numa lista, em vez de digitar.
