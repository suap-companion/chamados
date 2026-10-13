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
