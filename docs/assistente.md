# Assistente de redação ("Sugerir com IA")

O app ajuda a **escrever** os textos que a pessoa envia ao SUAP: **título**, **comentário**, **nota interna**, **suspensão**, **cancelamento**, **resolução**, **fechamento**, **reabertura** e a nota de **escalar/devolver**. A IA roda **no aparelho**; o contexto é **só o chamado aberto**; a pessoa **revisa e edita** o rascunho, e **nada é enviado ao SUAP sem a confirmação do diálogo**.

> Estado: marcos M0 a M6 do plano (spike, núcleo, fachada, interface, modelo local de reserva, base de documentos opcional, testes unitários e de instrumentação, lint e o passo no CI). A nuvem continua só planejada.

## Como se usa

Em cada diálogo de texto (Ações › Comentar, Nota interna, Resolver, Suspender, Cancelar, Reabrir, Fechar e avaliar; Gestão › Escalar e Devolver; o lápis do título) há o botão **Sugerir com IA**:

1. Se a pessoa já digitou algo no campo, isso é o "o que quero dizer" (a dica); se não, a IA parte só do chamado.
2. O texto aparece no campo, **editável**. Depois de um rascunho, há **Mais curto**, **Mais formal** e **Refazer** (usam o texto atual como base); **Parar** interrompe a geração.
3. Sem IA no aparelho, o botão coloca um **modelo pronto** com `[preencher]` para a pessoa completar, e avisa.

A seleção em lote (comentar/nota em vários chamados) não tem o botão: não há um único chamado de contexto.

## Arquitetura

```
Diálogos Compose ──▶ AssistButton (Kotlin)
                        │  state()/generate()            ┌─ chamados-assist (Rust, 100% testado)
                        ▼                                │   regras: privacidade, orçamento, prompt,
                  AnswerProvider ── AicoreProvider       │   limpeza da resposta, modelos prontos
                        │            (ML Kit GenAI)      │
                        └── assistPrompt / assistClean / assistTemplate (UniFFI, chamados-mobile)
```

- **`crates/chamados-assist`** (Rust, sem rede nem disco): `DraftKind`, `build_prompt`, `clean_output`, `template`. É igual para qualquer modelo e reutilizável pelo CLI.
- **`crates/chamados-mobile/src/assisting.rs`**: transforma o `TicketView` em contexto e expõe `assist_prompt`, `assist_clean` e `assist_template`.
- **`android/.../Assistant.kt`**: `AnswerProvider` (`state`, `download`, `generate`) e o `AicoreProvider` (Prompt API do ML Kit GenAI, `com.google.mlkit:genai-prompt:1.0.0-beta4`).
- **`android/.../AssistButton.kt`**: o botão, o streaming para o campo, os refinamentos e as mensagens de estado.

### Regras no Rust (testadas)

- **Privacidade interno × público:** as **notas internas só entram no contexto de textos internos** (nota interna, escalar, devolver). Em qualquer texto público (comentário, resolução, fechamento, cancelamento, suspensão, reabertura, título) elas nunca vão ao modelo.
- **Orçamento:** o pedido tem no máximo ~9000 caracteres (≈ 3000 tokens; a Prompt API aceita menos de 4000 tokens de entrada). Cabem a descrição (até 1500 caracteres), a dica, o rascunho anterior e, das entradas **mais novas para as mais antigas**, até 12 (600 caracteres cada); o que sobra fica de fora e o app avisa.
- **Fidelidade:** o prompt manda usar só os fatos do chamado e do que a pessoa disse, não inventar causas, datas, nomes nem números, e escrever `[preencher]` onde faltar dado.
- **Limpeza:** a resposta perde cercas de código, marcação, rótulos ("Texto:") e aspas; o título vira uma linha de até 120 caracteres; os demais textos, até 4000.

## Seleção do provedor

| Estado | O que acontece |
|---|---|
| IA do sistema pronta (`AVAILABLE`) | gera com o Gemini Nano via AICore |
| IA do sistema não pronta, **modelo local instalado** | gera com o modelo local (LiteRT-LM) |
| Modelo do sistema ainda não baixado (`DOWNLOADABLE`), sem modelo local | usa o modelo pronto e oferece **Baixar a IA do aparelho** (progresso em MB) |
| Baixando (`DOWNLOADING`), sem AICore ou aparelho sem suporte, sem modelo local | usa o modelo pronto |
| Assistente **desligado** em Configurações | usa o modelo pronto |
| Falha da IA | mensagem em português (ex.: AICore sem resposta, preparo em andamento) |

A escolha é feita a cada pedido (`ChainProvider` em `Assistant.kt`): primeiro a IA do sistema, depois o modelo local.

## Modelo local de reserva (opcional)

Em **Configurações › Assistente de IA** há o liga/desliga do assistente, o estado da IA do sistema e o modelo local, que **só é baixado com o consentimento da pessoa**.

- **Modelo:** Qwen2.5 1.5B Instruct em `.litertlm` (quantizado em 8 bits), do repositório `litert-community/Qwen2.5-1.5B-Instruct` no Hugging Face: **1,6 GB**, licença **Apache-2.0**, **sem login nem aceite de termos**. O Gemma, que era a ideia inicial, é *gated* (exige conta e aceite no Hugging Face), então não dá para baixá-lo de dentro do app. Constantes e SHA-256 em `LocalModel.kt`.
- **Consentimento:** antes de qualquer byte, um diálogo mostra o tamanho, a licença, a origem, o espaço livre (exige 20% de folga) e um aviso se o aparelho tem menos de 3 GB de memória; "Baixar só no Wi-Fi" vem marcado.
- **Download:** feito pelo `DownloadManager` do sistema (retoma sozinho, segue redirecionamentos, mostra a notificação e continua com o app fechado), na pasta externa do app (`Android/data/<app>/files/models`). A tela mostra o progresso e permite cancelar.
- **Verificação:** ao terminar, o app confere o **SHA-256** (e o tamanho); se não confere, o arquivo é apagado e a pessoa é avisada. Só o arquivo conferido é usado.
- **Uso:** o motor (`LocalLlm`) carrega o modelo na primeira sugestão (uns 10 segundos, ~2 GB de memória), fica carregado enquanto o diálogo de texto está aberto e é liberado quando ele fecha. Roda só na CPU.
- **Apagar:** "Apagar o modelo" remove o arquivo e libera os 1,6 GB; desinstalar o app também.
- **Não está no repositório** nem no backup (pasta externa do app).

> **Custo no APK:** a biblioteca LiteRT-LM (`litertlm-android` 0.16.1) traz ~20 MB para `arm64-v8a` e ~25 MB para `x86_64` (o emulador); o APK de depuração passou de ~51 MB para ~103 MB. Em aparelhos de 32 bits (`armeabi-v7a`) a biblioteca não existe e o modelo local não funciona (o app cai no modelo pronto). Versões mais novas da biblioteca (0.18) exigem Kotlin 2.4; com o Kotlin 2.2 do projeto, a última compatível é a 0.16.1.

- **Nuvem** é *planejada, não implementada*: ficaria **desativada por padrão**, com consentimento explícito, sem enviar tokens do SUAP, credenciais nem documentos. O MediaPipe LLM Inference está em manutenção, por isso o modelo local usa o LiteRT-LM.

## Documentos de apoio (opcional)

Além do chamado aberto, a pessoa pode importar **PDF, TXT e Markdown** (menu ⋮ › *Documentos de apoio*, ou Configurações › Assistente de IA). Ao escrever, o assistente busca nesses documentos os **trechos mais relevantes** para o chamado e os usa como apoio, **citando o arquivo e a página** (ex.: `(Manual.pdf, p. 3)`); a tela mostra "Documentos de apoio considerados: …". Sem documentos, ou sem trecho relevante, nada muda no pedido.

- **Arquitetura:** o crate `suap-rag` (Rust, do repo `suap-companion/suap-kit`) faz tudo e é reutilizável pelo CLI: extração de texto (TXT/MD em UTF-8 ou Latin-1; PDF por página com `pdf-extract`, **sem OCR**), divisão em trechos (~800 caracteres, parágrafos respeitados, 100 de sobreposição), **índice SQLite com FTS5/BM25** (sem acentos nem maiúsculas, prefixos para plurais) e esquema versionado (`PRAGMA user_version`). A fachada `Docs` (`chamados-mobile/src/docs.rs`) expõe `import_file`, `list`, `index_document`, `reindex`, `remove` e `search`; `IndexControl` dá o progresso e o cancelamento.
- **Busca:** as palavras da pergunta (sem as comuns do português) viram uma consulta FTS5; só entram documentos **prontos**, e um trecho precisa conter **metade das palavras** da pergunta (um trecho que casa só com uma entre várias é descartado). Vão ao modelo no máximo 3 trechos de 700 caracteres, dentro do orçamento do pedido.
- **Indexação:** em segundo plano pelo WorkManager (`IndexWorker`, um trabalho por documento), gravando **página a página**; o progresso é lido do banco. Cancelar ("Pausar") mantém o que foi gravado e a próxima rodada **continua da última página**, inclusive depois de o processo ser encerrado (a tela recoloca na fila o que ficou parado). Estados: na fila, indexando, pronto, sem texto (PDF digitalizado), protegido por senha, inválido, grande demais (50 MB / 500 páginas).
- **PDF malformado:** o leitor roda em `catch_unwind` e um pânico vira "arquivo inválido"; por isso o perfil `mobile` passou de `panic = "abort"` para `unwind`.
- **Remover / reindexar:** apagar remove o arquivo copiado, os trechos e o que a busca guardou (`optimize` do FTS5); reindexar esquece os trechos e indexa de novo.
- **Onde ficam:** pasta privada do app (`noBackupFilesDir/docs-<perfil>/`: `index.db` e `files/`), **fora do backup automático** (que tem limite de 25 MB e deixaria de funcionar com PDFs grandes), uma base por perfil; nunca vão ao SUAP, à sincronização nem a logs.
- **Custo:** as bibliotecas Rust passaram de ~4,4 MB para ~7,9 MB em arm64 (SQLite embutido e leitor de PDF).

## Permissões e privacidade

- Nenhuma permissão nova do app. A biblioteca declara `BIND_SERVICE` do AICore e a consulta ao pacote `com.google.android.aicore`.
- **Nada sai do aparelho** para gerar o rascunho; o texto só vai ao SUAP quando a pessoa confirma o diálogo. Não há persistência de rascunhos e o conteúdo dos chamados **não vai para logs**.
- As mensagens de falha nunca incluem texto do chamado.

## Compatibilidade (spike M0)

- A Prompt API é **beta** ("sem SLA nem política de descontinuação") e pede **minSdk 26** (Android 8.0). O app passou a ser **minSdk 26** também (antes era 24, Android 7.x), então não há mais `tools:overrideLibrary` nem checagem de versão; a biblioteca NDK do Rust também é compilada para a API 26.
- Precisa do app **AICore** e de **aparelho compatível** (a disponibilidade do Gemini Nano é limitada), sem bootloader desbloqueado; há **cota de inferência por aplicativo** (valores não publicados) e o limite de entrada é de **menos de 4000 tokens**.
- **Não testado em aparelho real:** o emulador do projeto não tem AICore, então só o caminho "sem IA" (modelo pronto) foi exercitado de ponta a ponta. A geração com o Gemini Nano, o **download do modelo local** (1,6 GB pelo `DownloadManager`), a verificação do SHA-256 em arquivo real e a **geração com o modelo local** foram escritos conforme as APIs publicadas (conferidas nos AARs) mas **não foram executados**: o diálogo de consentimento, o estado e a seção de Configurações foram vistos no emulador; o resto precisa de um aparelho real (com AICore para o Gemini Nano; com 3 GB ou mais de memória e arm64 para o modelo local), inclusive para avaliar a qualidade do texto em português. O Qwen2.5 1.5B é um modelo pequeno: espere mais erros que no Gemini Nano.

## Limitações conhecidas

- A IA pode errar ou inventar: por isso o prompt restringe aos fatos, há aviso na tela e a pessoa revisa antes de enviar.
- Contexto longo é truncado (as entradas mais antigas saem primeiro).
- Sem IA (nem do sistema nem o modelo local), "Mais curto/formal/refazer" não funcionam.
- Sem lote e sem OCR (PDF digitalizado fica "sem texto"); os documentos são só de apoio ao texto, não um chat de perguntas e respostas.
- A busca é por palavras (FTS5/BM25): não acha sinônimos. Busca semântica com embeddings é uma evolução possível (exige um modelo de embeddings baixado com consentimento).

## Como rodar e testar

- Rust: `cargo test -p chamados-assist (os testes do `suap-rag` rodam no repo `suap-kit`)` e, no Docker do projeto, `cargo llvm-cov --workspace --all-targets --ignore-filename-regex 'main\.rs' --fail-under-lines 100`.
- Android (unitários e lint): `sh scripts/build-android.sh`, depois `cd android && ./gradlew testDebugUnitTest lintDebug assembleDebug` (os testes ficam em `android/app/src/test`, rodam na JVM; o lint ignora o código gerado em `uniffi/` por `android/app/lint.xml`; o `android.yml` roda os dois antes de montar o APK).
- Android (instrumentação, com emulador ou aparelho conectado): `cd android && ./gradlew connectedDebugAndroidTest`. Os testes (`android/app/src/androidTest`) usam a biblioteca nativa de verdade: o botão "Sugerir com IA" com um provedor falso (IA pronta, sem IA, notas internas só em textos internos, refinamentos) e o fluxo de documentos (importar, indexar em segundo plano, buscar, citar arquivo e página no pedido, estados de erro, remover sem deixar nada). Eles só mexem em documentos com o prefixo `teste-instrumentacao-` e os apagam ao final. **O CI não os roda** (exigiria um emulador); rode-os antes de uma release.
- No emulador, abra um chamado e use **Sugerir com IA** (cai no modelo pronto sem AICore); para exercitar a IA de verdade, use um aparelho com AICore e Gemini Nano.
