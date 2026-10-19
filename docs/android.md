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

## Títulos e sincronização no app (M7)

- **Títulos locais**: aparecem na lista e são editados no detalhe do chamado (`Chamados.titles/setTitle/removeTitle`; mesmo `titles.json` do CLI).
- **Sincronização cifrada** (`chamados-mobile`, módulo `sync`): mesmo documento e mesma criptografia do CLI (perfis + títulos, XChaCha20-Poly1305), só com bucket `r2`/`s3` (uma pasta local não existe em outro aparelho). Tela: Configurações > Sincronização entre dispositivos.
- **Sem chaveiro no Android**: a chave fica em `config/sync.key` e as credenciais do bucket em `config/s3-credentials` (`0600`), no armazenamento privado do app; nunca vão no documento sincronizado. Envolver a chave com o Android Keystore fica como melhoria.
- **Pareamento**: `chamados sync key pair` (CLI) mostra um QR (e o texto `chamados-sync://pareamento?...`) com a **chave** e os dados **não secretos** do bucket (endpoint, bucket, região, prefixo). O app lê o QR (CameraX + ML Kit, modelo embutido; a câmera só é pedida nessa hora) ou aceita o texto colado. **As credenciais do bucket não vão no QR**: digite-as no app. O QR contém a chave: não o mostre em tela compartilhada. Trocar uma chave existente pede confirmação.
- **Perfil do app**: o app entra na sincronização com `updated_at = 0`, então as configurações de outro aparelho sempre valem mais e o app não muda o endereço do SUAP de um perfil já sincronizado.
- **Sincronização é manual** ("Sincronizar agora"); a periódica em segundo plano entra com o M6 (WorkManager).
- **Testado**: no emulador, com um S3 falso em `localhost` (`adb reverse tcp:9000 tcp:9000`; o `network_security_config` libera HTTP para `localhost` só para isso, e a fachada só aceita http em loopback): pareamento por texto, credenciais, título do CLI na lista do app e título do app no CLI. **Não exercitado**: a leitura real do QR (o emulador não tem câmera utilizável; sem câmera o leitor mostra o aviso para colar o texto) e um bucket R2 de verdade.
- **Dependências novas** (justificadas): `qrcode` (Rust, sem recursos de imagem) para desenhar o QR no terminal; `androidx.camera:*` 1.4.2 e `com.google.mlkit:barcode-scanning` 17.3.0 para ler o QR no app (o APK de depuração com as 3 ABIs passa de ~47 MB; o de release terá splits por ABI no M8).

## Acompanhamento em segundo plano (M6)

- **Fachada**: `Chamados.watchOnce(queue, todos, mensagens)` roda uma rodada do mesmo `watch` do CLI (`chamados-app::watching`) e devolve `WatchItem` (`new`, `status`, `left`, `message`) já com o título local. A primeira olhada de cada lista só registra o estado (`watch.json` do perfil). Só lê: nada é escrito no SUAP.
- **App**: `WorkManager` periódico (`WatchWorker`, 15/30/60/120 min, só com rede) em Configurações > Acompanhamento, com a lista (Meus/Suporte), mensagens novas opcionais e "Verificar agora". Cada mudança vira uma notificação (acima de 5, um resumo). Na tela bloqueada o conteúdo do chamado não aparece (visibilidade privada).
- **Sessão expirada**: um aviso "sessão expirada" (uma vez por expiração, rearmado quando a sessão volta); o login continua só pela WebView.
- **Permissão**: `POST_NOTIFICATIONS` (Android 13+) é pedida ao ligar o acompanhamento; sem ela o acompanhamento roda mas não avisa.
- **Limites**: sem push (exigiria servidor nosso), a latência é o intervalo; o mínimo do Android é 15 min e o Doze/economia de bateria podem atrasar; falhas de rede ou do SUAP são silenciosas e tentadas na próxima rodada.
- **Testado no emulador**: baseline sem aviso; chamado criado pelo CLI gerou a notificação "Novo"; apagar o arquivo de sessão gerou o aviso de sessão expirada. **Não exercitado**: o agendamento periódico real (esperar o intervalo; só a verificação imediata foi usada), mudança de situação e mensagens novas no aparelho (cobertas pelos testes Rust).
- **Dependência nova**: `androidx.work:work-runtime-ktx` 2.10.0 (agendamento que sobrevive a reinício e respeita o Doze; a alternativa com AlarmManager + serviço é mais frágil).

## Arquivar chamados (#102)

- **O que é**: uma marca só nossa, como o título local. O SUAP não tem esse estado e **nada é enviado a ele**. Fica em `archive.json` do perfil e viaja pela sincronização cifrada (campo `archived` do documento, mesma regra de mescla dos títulos: vale a mudança mais recente; desarquivar deixa uma marca "desarquivado").
- **CLI**: `chamados archive <id>...` e `chamados unarchive <id>...`; `list` esconde os arquivados e `list --arquivados` mostra só eles (com os mesmos filtros e `--titulo`); `show` avisa "Arquivado: sim"; `watch` não avisa dos arquivados (o estado deles continua sendo registrado, então desarquivar não gera enxurrada).
- **App**: botão Arquivar/Desarquivar no detalhe; chip "Arquivados" na lista (da fila escolhida, com todas as situações) e busca pelo título local; o acompanhamento em segundo plano ignora os arquivados.
- **Limite**: a lista de arquivados é a da fila escolhida filtrada pela marca, então um arquivado que já saiu dessa lista do SUAP não aparece (continua marcado). Clientes anteriores à 0.35 ignoram o campo `archived` e o perdem se enviarem o próprio documento: atualize CLI e app juntos.

## Tema do Design System do gov.br (#103)

- **Referência**: Design System do Governo Federal (https://www.gov.br/ds/), versão estável 3.x (`@govbr-ds/core`). O site oficial é renderizado por JavaScript e não pôde ser lido direto; os valores foram conferidos num espelho dos tokens oficiais (`@govbr-ds/core` 3.7.0, repositório de terceiros `jonasluz/govbr-ds-kit`). Vale reconferir com o pacote oficial antes de uma mudança de marca (a v4 do DS muda o azul para `#0040B4` e a fonte para Noto Sans).
- **Onde**: `Theme.kt` (`ChamadosTheme`, usado por `MainActivity` e `ShareActivity`). Paleta do DS ligada aos papéis do Material 3: azul de interação `#1351B4` (`--interactive`), texto `#333333`, fundo branco, cinzas `#F0F0F0`/`#E6E6E6`, sucesso `#216E1F`, tema escuro com fundo `#071D41` (`--background-dark`) e destaque `#C5D4EB` (`--interactive-dark`). Cantos de 4, 8 e 12 dp; entrelinha 1,45 no texto corrido.
- **Contraste (WCAG AA)**: todos os pares texto/fundo do tema claro e do escuro passam de 4,5:1 e as bordas de 3:1 (menor: 5,97:1 no texto secundário sobre o cartão claro; bordas 3,54:1 no claro). Por isso o erro usa `#B50909` (6,98:1) e não o `--danger` `#E52207` (que fica abaixo de 4,5:1 sobre branco).
- **Fonte**: o DS usa Rawline; **não foi embutida**, pois falta confirmar a licença de redistribuição (e baixar a fonte exige sua autorização). Enquanto isso, a fonte do sistema.
- **Não feito**: o contorno de foco tracejado dourado do DS (relevante só para teclado/acessibilidade no Android) e os componentes próprios (cabeçalho com logo, brasão). Alvos de toque seguem o mínimo de 48 dp do Material 3.
- **Testado**: lista e detalhe no emulador, claro e escuro (`adb shell cmd uimode night yes|no`).

## Exportar chamado como ZIP (#104)

- **Conteúdo** (`chamados-app::export`): `chamado.md` (campos, linha do tempo e anexos em texto legível), `chamado.json` (os mesmos dados estruturados, com `local_title`, `archived` e o caminho de cada anexo) e, em `anexos/`, os anexos com nomes seguros e sem repetição (as mesmas regras do `download`). Não leva cookies, sessão nem dados de outros chamados.
- **CLI**: `chamados export <id> [-o pasta|arquivo.zip] [--sem-anexos] [--force]` (grava `chamado-<id>.zip`; não sobrescreve sem `--force`).
- **App**: botão "Exportar como ZIP" no detalhe; pergunta se leva os anexos e entrega o arquivo pelo "Compartilhar" do Android (a cópia fica em cache, uma por vez). `Chamados.exportTicket(id, anexos, pasta)` na fachada.
- **ZIP sem dependência**: escritor mínimo (`zip.rs`: arquivos "stored", nomes UTF-8, CRC-32, sem ZIP64: até 4 GB e 65535 arquivos, com erro claro além disso); conferido com o `zipfile` do Python.
- **Testado**: no emulador (exportar o #10 com anexos e ver o seletor de compartilhamento com `chamado-10.zip`, e abrir o ZIP gerado pelo app). Não exercitado: entregar a um app de destino de verdade e chamado com anexos reais no aparelho (cobertos por testes Rust com SUAP simulado).

## Adiar chamados — "snooze" (#109)

- **O que é** (como o Adiar do Spark): um arquivar que desarquiva sozinho. O chamado some das listas e do acompanhamento até a data escolhida; nessa hora volta **ao topo da lista, marcado "↩ voltou do adiamento"**, e a pessoa é avisada. Ver o chamado (abrir o detalhe ou `chamados show`) tira a marca. É uma marca só nossa (o SUAP não tem esse estado; **nada é enviado a ele**): `snooze.json` do perfil, sincronizada cifrada (campo `snoozed` do documento, mesma regra de mescla das outras marcas; a sincronização e o cancelamento valem em todos os aparelhos).
- **Atalhos de data** (os do Spark): mais tarde (+3 h), amanhã 8h, fim de semana (sábado 9h), próxima semana (segunda 8h) ou data e hora à escolha. Dá para **mudar a data** ou **cancelar** o adiamento. Máximo de um ano.
- **App**: "Adiar…" no detalhe; chip **"Adiados"** com a lista própria (do que volta antes ao que volta depois, com a hora de cada um e busca pelo título local); os que voltaram aparecem no topo das listas normais. O aviso de "voltou" vem de uma verificação **local, sem rede**, a cada 15 minutos (e também a cada rodada do acompanhamento); cada chamado é avisado uma vez só, e a permissão de notificações é pedida na primeira vez que se adia algo.
- **CLI**: `chamados snooze <id>... [--em 3h | --ate "AAAA-MM-DD HH:MM" | --amanha | --fim-de-semana | --semana]` e `chamados unsnooze <id>...`; `list` esconde os adiados e mostra os que voltaram primeiro, com "↩"; `list --adiados` mostra os que esperam, com a data; `show` informa a situação; `watch` não avisa dos adiados e anuncia, uma vez, quando um volta. Fuso: **UTC-03:00 por padrão**, mudado com `--fuso` ou `CHAMADOS_FUSO` (ex.: `--fuso=+05:30`); as datas são calculadas sem biblioteca de calendário (o fuso é fixo, sem horário de verão).
- **Limites**: sem "Someday" (adiar sem data); uma mensagem nova num chamado adiado não o traz de volta sozinha (o acompanhamento não avisa enquanto ele está adiado). Quem já tinha sincronização com versões anteriores à 0.38 perde as marcas de adiamento se elas enviarem o próprio documento: atualize CLI e app juntos. A aba "Adiados" mostra os adiados que ainda estão na lista do SUAP da fila escolhida.
- **Testado**: no emulador (adiar com "Mais tarde", chamado sumindo de "Meus", "Adiado até…" no detalhe com Alterar/Cancelar, voltar ao topo marcado e a notificação "Chamado #5 voltou"). **Não exercitado**: o seletor de data e hora ("Escolher data e hora…") até o fim, a verificação periódica real (esperar os 15 minutos; só a rodada do acompanhamento foi usada) e a sincronização entre CLI e app no aparelho (coberta por testes Rust).

## APK assinado na release (M8)

- **Versão**: `versionName`/`versionCode` do app vêm do `Cargo.toml` (`0.39.0` → código 3900): uma única fonte; basta subir a versão do workspace.
- **Workflow `android.yml`** (reaproveitado pela release e também **testável à mão**: Actions > Android > Run workflow, que compila e assina sem publicar nada e deixa o APK como artefato `apk`) e job `android-release` do `release.yml`, que anexa o APK à release. O que o build faz (roda quando a release é publicada): instala o NDK 29, compila a biblioteca Rust das 3 ABIs com `cargo-ndk`, gera as ligações Kotlin, roda `./gradlew assembleRelease`, confere a assinatura com `apksigner verify` e anexa `chamados-<tag>-android.apk` à release. **Sem a chave de assinatura o APK é compilado mas não é publicado** (aviso no log): nunca se publica APK sem assinar, nem assinado com chave de teste.
- **Chave de assinatura** (uma vez, por quem é dono do app): `sh scripts/android-keystore.sh` cria `chamados-release.jks` (RSA 4096, 10 mil dias) e mostra os comandos para cadastrar os segredos `ANDROID_KEYSTORE_B64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` e `ANDROID_KEY_PASSWORD` no repositório. **Guarde o arquivo e as senhas num cofre**: sem a mesma chave, uma versão nova não instala por cima da anterior (é preciso desinstalar, perdendo os dados do app). `*.jks` está no `.gitignore`.
- **Build local assinado** (para testar): exporte `ANDROID_KEYSTORE_FILE`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` e `ANDROID_KEY_PASSWORD` e rode `./gradlew assembleRelease` em `android/` (depois de `sh scripts/build-android.sh`).
- **Chave atual**: criada em 2026-10-15 (RSA 4096, PKCS12, alias `chamados`, 10 mil dias) e cadastrada nos segredos do repositório. Impressão digital SHA-256 do certificado (pública; serve para conferir que um APK é o seu): `EC:7A:C4:10:83:D5:23:6B:21:DE:C2:AC:61:B9:50:03:FE:C7:EE:40:CF:F2:4A:B6:92:57:C5:12:0C:1B:16:77`. O arquivo e a senha ficam **só com o dono**, fora do repositório (o GitHub não deixa ler um segredo de volta: guarde uma cópia no cofre).
- **Instalar o APK**: baixe `chamados-<tag>-android.apk` da release no aparelho e abra o arquivo (o Android pede para permitir instalar de fontes desconhecidas para o navegador/gerenciador de arquivos usado). Atualizações: instale o APK novo por cima.
- **Tamanho**: ~45 MB (3 ABIs da biblioteca Rust + ML Kit + CameraX; sem minificação, pois JNA e UniFFI usam reflexão). Separar por ABI (~15 MB cada) fica como melhoria.
- **Testado**: `assembleRelease` assinado com uma chave de teste temporária (descartada), `apksigner verify` ok e o APK instalado e aberto no emulador. **Não validado**: o job do workflow em si, que só roda de verdade numa release (o `act` não reproduz o SDK/NDK do Android); a primeira release com a chave cadastrada é o teste real.
- **Workflows**: actions atualizadas (`checkout@v7`, `softprops/action-gh-release@v3`, `setup-java@v6`) e `dependabot.yml` agrupando as atualizações.

## Cabeçalho no padrão do Design System do gov.br (#113)

- **O que é** (`Header.kt`, `GovBrHeader`): o componente "Header" do DS (https://www.gov.br/ds/components/header), usado em todas as telas no lugar das barras antigas: **logo** (uma marca "C" do app, que será trocada pelo logo de verdade), **título** em uma linha com reticências, **☰ menu**, **⋮ acesso rápido**, **grade de funcionalidades**, **busca (lupa)** e **conta** (avatar com seta). Menus suspensos com título em caixa alta e itens de 48 dp; sombra do header; claro e escuro (tokens da #103).
- **No app**: lista de chamados — ☰ abre "LISTAS" (Meus, Suporte, A fechar, Adiados, Arquivados), o título mostra a lista atual, ⋮ leva a Configurações e Sincronização, a grade tem Novo chamado e Atualizar, a lupa abre o campo de busca (já com foco; ✕ fecha) quando a lista tem busca (fila de suporte, adiados e arquivados), e o menu da conta tem "Sair". Telas de detalhe, novo chamado, sincronização e configurações trocam o ☰ por uma seta de voltar. No login, ⋮ leva ao endereço do SUAP. O subtítulo só aparece em telas largas (≥ 600 dp), como no DS.
- **Mudanças de interface**: os chips das listas e o campo de busca fixo saíram da tela (viraram o ☰ e a lupa); os botões "Novo", "Atualizar" e "Config." viraram itens do cabeçalho.
- **Dependência nova**: `androidx.compose.material:material-icons-core` (só o conjunto básico de ícones: menu, busca, fechar, voltar...; o "extended" é grande e não foi usado). A grade de funcionalidades é desenhada à mão.
- **Fora do escopo**: logo e brasão oficiais, fonte Rawline e o menu lateral completo (drawer).
- **Testado no emulador**: lista (Meus e Suporte), menu ☰, lupa e campo de busca, detalhe em claro e escuro. **Não exercitado** em tela larga (tablet, com subtítulo) nem com leitor de tela (TalkBack); os ícones têm descrição de conteúdo.

## Perfis, conta, ícone e tela inicial (#115)

- **Foto, nome e matrícula do SUAP** (`chamados-core::profile`, `Chamados.cachedUser/refreshUser`): lidos do canto da página do SUAP (`#user-tools a.user-profile`: foto, nome usual e `title="Vínculo: matrícula"`) e guardados no aparelho (`usuario.txt`, `foto.img`) para o cabeçalho funcionar sem rede. A foto só é baixada do mesmo SUAP do perfil (a sessão nunca vai a outro servidor) e a imagem padrão do SUAP ("sem foto") não conta; sem foto, o avatar mostra o ícone de pessoa. Sair da conta apaga o que foi guardado.
- **Foto 3x4 em círculo 1x1, como o SUAP**: o CSS do SUAP (`div.photo-circle`) dá à imagem a largura do círculo, encostada no topo e um pouco acima (margem negativa); no app, a imagem é recortada pelo círculo alinhada ao topo (`BiasAlignment(0, -0,6)`), o que põe o rosto dentro.
- **Menu da conta** (avatar): foto, nome usual, matrícula e **perfil em uso**, depois "Trocar para …" (um item por outro perfil), "Cadastrar novo perfil…" e "Sair".
- **Perfis do app** (`Profiles.kt`): ambientes com nome e endereço do SUAP (por exemplo "IFRN" e "Local (emulador)"), cada um com **sessão e dados próprios** (pasta própria). O primeiro uso cria o perfil do endereço que já estava em uso, na mesma pasta (nada se perde). Trocar de perfil não apaga nada; o login de um perfil novo é feito na WebView. "Endereço do SUAP", em Configurações, muda o endereço do perfil em uso. O acompanhamento em segundo plano e os avisos de adiados olham só o perfil em uso. Sem editar nem excluir perfis por enquanto.
- **Cabeçalho**: sem a grade de "funcionalidades" (alternar aplicações, que o app não tem); o ⋮ ficou com Configurações e Sincronização; sem o ☰ na tela inicial.
- **Tela inicial**: três **abas** (Meus, Suporte, A fechar), a situação dentro de cada aba (**Ativos, Adiados, Arquivados**, que não são abas), botão **`+`** fixo (Novo chamado) e **puxar para atualizar**. Na aba "A fechar" o SUAP só aceita a paginação: adiados e arquivados valem pelas marcas locais.
- **Ícone e splash**: o ícone do app vem de `docs/icone.svg` (e `docs/icone.png`): ícone adaptável com o **símbolo sobre o verde-petróleo cheio** (`ic_launcher_foreground.xml`, vetor feito do SVG, dentro da zona segura de 66 dp, com o ponto verde-menta mais perto do balão), para ficar bom em qualquer máscara do lançador (círculo, quadrado, gota); versões PNG de vários tamanhos para o Android antigo. O logo do cabeçalho é o `icone.png`; `docs/logo.png` é a tela de carregamento do app (enquanto a sessão é conferida). No Android 12+ o sistema mostra antes o ícone dele, como sempre. Sem ícone temático (monocromático) por enquanto: o sinal de terminal precisaria ser um recorte.
- **Testado no emulador**: abas, chips, `+`, ícone novo, e o menu da conta com nome "Pessoa 0", matrícula e perfil "Local (emulador)". **Não exercitado**: foto de verdade (o SUAP local só tem a foto padrão), trocar e cadastrar perfil até o fim, puxar para atualizar, e o tablet.

## Nome do repositório (#118)

O repositório passou de `suap-companion/chamados-cli` para **`suap-companion/chamados`**: ele agora tem a biblioteca, o CLI e o app Android. O GitHub redireciona o endereço antigo (clones, links e releases), mas atualize o remoto local: `git remote set-url origin git@github.com:suap-companion/chamados.git`. O nome do binário (`chamados`), dos crates e dos arquivos das releases não mudou; os segredos da chave do APK e os campos de horas acompanharam o repositório.

## Selos de quantidade (#117)

Os filtros de cada aba (Ativos, Adiados, Arquivados) mostram um selo com quantos chamados há em cada situação (acima de 99, "99+"; sem selo quando é zero). A conta vem da fachada (`list_counts`): Ativos usa a lista de sempre da aba; Adiados e Arquivados contam em todas as situações do SUAP, como as listas deles (na aba "A fechar", que só pagina, a mesma lista serve às três). Os números se atualizam a cada atualização da lista, inclusive ao arquivar, adiar ou desfazer; se a contagem falhar, os selos ficam como estavam.

## Listagem no padrão gov.br: Lista, Item e Loading (#119)

A lista de chamados usa o padrão do Design System: itens (`br-item`) numa lista (`br-list`) com divisores finos, padding de 16 dp, alvo de toque de 48 dp, avatar redondo e um único alvo de leitor de tela por item ("chamado 5, Aberto, título…"); o estado "selecionado" já existe (para a seleção em lote, #122). Na primeira carga de uma aba, o **skeleton screen** (mesma forma dos itens, barras que pulsam, parado se o sistema reduz animações) substitui o círculo de progresso; com itens na tela, o puxar para atualizar basta. Vazio e erro têm mensagem curta, e o erro, a ação "Tentar de novo". Código em `TicketList.kt`. O avatar mostra a foto de quem pediu, que vem na própria lista do SUAP (`TicketSummary.photo`) e é baixada uma vez e guardada no aparelho (`ticket_photo`); sem foto, ou sendo a padrão, fica o ícone de pessoa.

## Paginação infinita (#123)

Ao rolar até perto do fim da lista, o app carrega a página seguinte do SUAP (com o skeleton no rodapé enquanto carrega), sem repetir chamados e mantendo a aba, a situação e a busca. Páginas em que tudo já estava na lista, ou estava arquivado/adiado, não contam: o app tenta as seguintes (até 5 seguidas); uma página vazia, ou recusada por estar além do fim, encerra a lista com "Fim da lista." Atualizar a lista (puxar, trocar de aba, situação ou perfil) volta à primeira página.

## Seleção em lote (#122)

Um toque longo num chamado da lista inicia a seleção (o avatar vira um visto e o item fica realçado); com a seleção ativa, um toque marca ou desmarca, e a voltar do sistema cancela. A barra de ações mostra quantos estão escolhidos, "Selecionar tudo" e as ações para todos eles: **Arquivar** (ou **Desarquivar**, nos arquivados), **Adiar**, **Comentar**, **Nota interna** e **Tags**. As ações rodam um chamado por vez; um que falha não impede os outros. Ao terminar, um resumo diz quantos deram certo e o motivo de cada falha, e só os que falharam continuam selecionados, para tentar de novo. A seleção some ao trocar de aba, de situação ou de perfil.

## Fonte Rawline (#124)

O app usa a **Rawline**, a família tipográfica do Design System gov.br, nos quatro pesos do DS (400 Regular, 500 Medium, 600 SemiBold e 700 Bold), aplicada a todos os estilos do tema (`Theme.kt`, `GovBr.Rawline`). Os arquivos `.ttf` (em `android/app/src/main/res/font/`) vieram do CDN oficial do DS (`cdngovbr-ds.estaleiro.serpro.gov.br/design-system/fonts/rawline/`; o pacote npm `@govbr-ds/core` só referencia o CDN). A Rawline é distribuída sob a **SIL Open Font License 1.1** (Copyright 2016 Henrique Ibaldo, Reserved Font Name RAWLINE): ela permite embutir e redistribuir a fonte, desde que o aviso e a licença acompanhem as cópias (`docs/licencas/Rawline-OFL.txt`) e que a fonte não seja vendida isoladamente nem alterada com o mesmo nome. Custo: cerca de 1 MB a mais no APK.

## Detalhe do chamado no padrão do DS gov.br (#129)

O cabeçalho do detalhe segue o DS: o **título local é o H1** (sem título, o H1 é o tipo do chamado); com título, o **tipo vem abaixo, em itálico**; ao lado do H1 há o botão de **editar o título**, que abre um diálogo para salvar, remover (texto vazio) ou desistir; a situação é uma etiqueta colorida (`br-tag`) e os dados do chamado são itens com rótulo pequeno, valor e divisor fino (`DetailHeader.kt`). O campo e o botão "Título local" deixaram de existir.

## Ações em menus suspensos (#131)

Os botões do detalhe viraram três menus suspensos logo abaixo do título e da situação (`Actions.kt`): **Ações** (as da situação atual: assumir, iniciar, comentar, nota interna, anexar, resolver, suspender, cancelar, reabrir, fechar), **Gestão** (atribuir, escalar, devolver, reclassificar, tags e interessados) e **Mais** (o que é só seu: arquivar/desarquivar, adiar, alterar ou cancelar o adiamento e exportar como ZIP). Cada ação mantém a confirmação, os diálogos e o resultado de antes; o resultado da última ação aparece abaixo dos menus. A tela abre direto no conteúdo.

## Interessado com foto, nome e setor (#130)

No detalhe, o interessado aparece como no cabeçalho da conta: avatar redondo com a **foto**, ao lado o **nome usual** e, abaixo, o **setor** (`InterestedItem`). Tudo vem da página do chamado no SUAP: a foto é a do bloco da pessoa (`.person .photo-circle img`, lida em `TicketDetails.interested_photo`, só do mesmo SUAP e sem a foto padrão), o nome é o campo "Interessado" e o setor é o campo "Setor atual" (o SUAP não tem um campo "campus" separado nessa página). A foto é baixada uma vez e guardada no aparelho (`ticket_photo`); sem foto, fica o ícone de pessoa.

## Notas internas, anexos e a linha do tempo em abas (#132)

A linha do tempo do detalhe tem quatro abas, cada uma com a quantidade (`ThreadTabs.kt`): **Tudo** (comentários, acontecimentos, anexos e notas juntos, do mais novo ao mais antigo), **Comentários**, **Notas internas** e **Anexos**. A aba de notas só existe para quem é da equipe de atendimento (é quando a página do SUAP traz a aba `notas_internas`; para os demais ela some). Cada entrada mostra quem escreveu (foto e nome), o que foi (comentou, escreveu uma nota interna, anexou um arquivo ou um acontecimento, em itálico) e a data; as notas ficam em destaque. Os anexos mostram quem anexou e quando (da aba de anexos do SUAP) e baixam com um toque.

No núcleo, `TicketDetails` ganhou `thread` e `notes` (com o tipo `EntryKind`, o autor, a foto, o texto sem o título e uma data ordenável) e os anexos ganharam `by`; o CLI continua usando `timeline`, que não mudou.

## Ordem da linha do tempo (#133)

Acima das entradas há um botão que alterna a ordem: **↓ Mais novos primeiro** (padrão, como o SUAP mostra) ou **↑ Mais antigos primeiro**. A escolha vale para todos os chamados, é guardada no aparelho (preferência `timeline_newest_first`, `CoreAccess.newestFirst`) e continua depois de fechar e abrir o app; ela se aplica às abas Tudo, Comentários e Notas internas (a aba Anexos mantém a ordem do SUAP, que não traz uma data por anexo).

## Central de ajuda (#139)

O site de suporte ao usuário final fica em `site/` (HTML estático, sem build) e é publicado no GitHub Pages em https://suap-companion.github.io/chamados/ pelo workflow `.github/workflows/pages.yml` (ao mesclar mudanças de `site/` na `main`; também à mão em Actions › Pages). Os links de download e a versão vêm da API do GitHub (release mais recente) por `site/assets/site.js`, com fallback para a página de releases; assim, uma release nova não exige mudar o site. As capturas de tela do app ficam em `site/assets/img/`.
