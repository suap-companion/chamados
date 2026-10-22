# Assistente de redação ("Sugerir com IA")

O app ajuda a **escrever** os textos que a pessoa envia ao SUAP: **título**, **comentário**, **nota interna**, **suspensão**, **cancelamento**, **resolução**, **fechamento**, **reabertura** e a nota de **escalar/devolver**. A IA roda **no aparelho**; o contexto é **só o chamado aberto**; a pessoa **revisa e edita** o rascunho, e **nada é enviado ao SUAP sem a confirmação do diálogo**.

> Estado: marcos M0 a M6 do plano (spike, núcleo, fachada, interface, base de documentos opcional, testes unitários e de instrumentação, lint e o passo no CI). O modelo local de reserva foi **removido** (#165). A nuvem continua só planejada (#144).

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
| Modelo do sistema ainda não baixado (`DOWNLOADABLE`) | usa o modelo pronto e oferece **Baixar a IA do aparelho** (progresso em MB) |
| Baixando (`DOWNLOADING`), sem AICore ou aparelho sem suporte | usa o modelo pronto |
| Assistente **desligado** em Configurações | usa o modelo pronto |
| Falha da IA | mensagem em português (ex.: AICore sem resposta, preparo em andamento) |

A escolha é feita a cada pedido (`ChainProvider` em `Assistant.kt`): o primeiro provedor da lista que estiver pronto. Hoje só há a IA do sistema; a lista deixa espaço para outro provedor (a IA na nuvem com a chave da pessoa, #144).

## Modelo local (removido)

Até a v0.54.8 o app podia baixar, com consentimento, um modelo local de reserva (Qwen2.5 1.5B, 1,6 GB, LiteRT-LM). Ele foi **removido na v0.54.9 (#165)**: poucos aparelhos o rodam bem, nos testes em aparelho real ele nunca respondeu direito (a #146, que investigava isso, foi cancelada) e a biblioteca (LiteRT-LM) pesava dezenas de MB por ABI no APK (o APK de depuração caiu de ~109 MB para ~61 MB).

- **Quem já baixou o modelo** o perde na primeira abertura da versão nova: o app cancela o download pendente e apaga o arquivo (`Android/data/<app>/files/models`) e o cache do motor, liberando os 1,6 GB (`Assistant.removeLeftoverLocalModel`).
- **Ficam**: a IA do sistema (Gemini Nano, onde existir), o modelo pronto com `[preencher]` e os documentos de apoio.
- **Nuvem** é *planejada, não implementada* (#144): ficaria **desativada por padrão**, com consentimento explícito, usando a chave de API da própria pessoa, sem enviar tokens do SUAP, credenciais nem documentos.

## IA no CLI (`--AI`)

O CLI reutiliza as regras do `chamados-assist` (privacidade interno × público, orçamento, limpeza), mas com **provedores de nuvem escolhidos por perfil** (`chamados profile ia`): Gemini, Anthropic e a API `chat/completions` da OpenAI (também falada por Ollama e OpenRouter). Não há modelo local no CLI.

- **`crates/chamados-app/src/ai.rs`**: `AiSettings` (o `ia.json` do perfil, sem a chave), `draft_prompt`/`describe_prompt` (contexto a partir do `TicketDetails`), `generate` (HTTP com `reqwest`, 120 s) e `clean`. A chave vem da variável de ambiente do perfil (`GEMINI_API_KEY`, `ANTHROPIC_API_KEY`, `OPENAI_API_KEY` ou `--chave-env`) e nunca é gravada.
- **Tipos de texto**: além dos do app, `Reclassify` (justificativa) e `Describe` (descrição de um chamado a abrir, sem chamado como contexto).
- **Comandos**: `--AI [instrução]` em `title`, `comment`, `note`, `suspend`, `resolve`, `close`, `cancel`, `reopen`, `escalate`, `return`, `reclassify` e `open`; a instrução pode ser literal, `-` (entrada padrão) ou `@arquivo`. O texto gerado é mostrado e enviado em seguida. Veja o README.
- **Privacidade**: o chamado sai da máquina só nos comandos com `--AI`; notas internas só entram em textos internos; erros nunca levam texto de chamado.

## Documentos de apoio (opcional)

Além do chamado aberto, a pessoa pode importar **PDF, TXT e Markdown** (menu ⋮ › *Documentos de apoio*, ou Configurações › Assistente de IA). Ao escrever, o assistente busca nesses documentos os **trechos mais relevantes** para o chamado e os usa como apoio, **citando o arquivo e a página** (ex.: `(Manual.pdf, p. 3)`); a tela mostra "Documentos de apoio considerados: …". Sem documentos, ou sem trecho relevante, nada muda no pedido.

- **Arquitetura:** o crate `suap-rag` (Rust, do repo `suap-companion/suap-kit`) faz tudo e é reutilizável pelo CLI: extração de texto (TXT/MD em UTF-8 ou Latin-1; PDF por página com `pdf-extract`, **sem OCR**), divisão em trechos (~800 caracteres, parágrafos respeitados, 100 de sobreposição), **índice SQLite com FTS5/BM25** (sem acentos nem maiúsculas, prefixos para plurais) e esquema versionado (`PRAGMA user_version`). A fachada `Docs` (`chamados-mobile/src/docs.rs`) expõe `import_file`, `list`, `index_document`, `reindex`, `remove` e `search`; `IndexControl` dá o progresso e o cancelamento.
- **Busca:** as palavras da pergunta (sem as comuns do português) viram uma consulta FTS5; só entram documentos **prontos**, e um trecho precisa conter **metade das palavras** da pergunta (um trecho que casa só com uma entre várias é descartado). Depois, o **corte de relevância** da fachada (`relevant` em `chamados-mobile/src/docs.rs`, #169) exige **ao menos duas palavras** da pergunta no trecho (uma só, se a pergunta tem uma) e pontuação de pelo menos **metade da do melhor**: o índice sempre devolve os "melhores" trechos, mesmo quando nenhum serve, e uma pergunta que nenhum documento responde (por exemplo "reiniciar a impressora" num corpus sem nada sobre isso) agora **não gera referência**. No corpus real de teste (44 documentos do IFRN) as consultas boas continuaram trazendo o documento certo. Vão ao modelo no máximo 3 trechos de 700 caracteres, dentro do orçamento do pedido.
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
- **Teste em aparelho real:** feito num Moto G56 5G (Android 16) na v0.54.7; o resultado está na seção "Teste em aparelho real" abaixo. O emulador do projeto não tem AICore, e o aparelho de teste também não: a geração com o **Gemini Nano não será testada em aparelho real** (poucos usuários têm o Gemini Nano nativo; a #142 foi cancelada).

## Teste em aparelho real (#142)

**Aparelho:** Moto G56 5G, Android 16, arm64-v8a, 7,4 GB de RAM. **App:** v0.54.7 (oficial), com arquivos de teste e uma base real de 44 documentos Markdown (~340 KB) sobre procedimentos do IFRN.

| Item | Resultado |
|---|---|
| Gemini Nano (AICore) | AICore **ausente**: a configuração mostra "indisponível neste aparelho" e o botão cai no **modelo pronto** com `[preencher]`, com a mensagem em português. A geração com o Gemini Nano não pôde ser testada. |
| Modelo local (Qwen2.5 1.5B) | O download de 1,6 GB (~11 min no Wi‑Fi) e o SHA‑256 funcionaram, mas o app fechava ao terminar a geração (corrigido) e a resposta era sempre a mesma frase em inglês, sem relação com o chamado. **Removido** (#165, v0.54.9). |
| TXT e Markdown pequenos | Pronto, 1 trecho cada |
| PDF de 40 páginas (texto) | Pronto, 200 trechos, em menos de 3 s |
| PDF de 480 páginas (2,4 MB) | Pronto, 3.360 trechos, em menos de 3 s |
| PDF com mais de 500 páginas | "Grande demais (no máximo 50 MB e 500 páginas)" |
| PDF só com imagem | "Sem texto para ler… o app não faz OCR" |
| Arquivo inválido com extensão `.pdf` | "Arquivo inválido ou danificado" |
| 44 Markdown reais de uma vez | todos Prontos em até 5 s (443 trechos) |
| Remover documento | funciona (diálogo e confirmação) |

**Qualidade da busca** (mesmo motor, rodado no PC sobre os mesmos 44 arquivos): em 8 consultas, 7 trouxeram o documento certo nos 3 primeiros trechos ("SSO Moodle autenticação SUAP" → `sso-moodle-auth-suap.md` em 1º; "restaurar disciplina modelo" → `disciplinas-modelos-restauracao.md`). A falha encontrada foi a **falta de corte de relevância**: "reiniciar a impressora" (nenhum documento trata disso) devolvia 3 trechos sem relação. Corrigida na #169 (ao menos duas palavras da pergunta no trecho e pontuação de pelo menos metade da do melhor).

**Não foi testado**, e por quê:
- **PDF protegido por senha:** não havia um arquivo para o teste (a fixture é a #172).
- **Retomada depois de matar o app:** a indexação é rápida demais (480 páginas em menos de 3 s) para ser interrompida no meio; segue coberta pelos testes automatizados.
- **Citação de arquivo e página no rascunho:** só aparece com a IA do sistema pronta (sem AICore, o modelo pronto não usa os documentos).
- **PDFs digitalizados grandes ou com muitas fontes:** só PDFs sintéticos.
- **Gemini Nano:** decisão de não testar (ver acima). Quem não tem IA nativa passa a ter a IA na nuvem com a chave da própria pessoa (#144).

## Limitações conhecidas

- A IA pode errar ou inventar: por isso o prompt restringe aos fatos, há aviso na tela e a pessoa revisa antes de enviar.
- Contexto longo é truncado (as entradas mais antigas saem primeiro).
- Sem a IA do sistema, "Mais curto/formal/refazer" não funcionam.
- Sem lote e sem OCR (PDF digitalizado fica "sem texto"); os documentos são só de apoio ao texto, não um chat de perguntas e respostas.
- A busca é por palavras (FTS5/BM25): não acha sinônimos; trechos que só compartilham duas palavras genéricas com a pergunta ainda podem passar no corte de relevância. Busca semântica com embeddings é uma evolução possível (exige um modelo de embeddings baixado com consentimento).

## Como rodar e testar

- Rust: `cargo test -p chamados-assist (os testes do `suap-rag` rodam no repo `suap-kit`)` e, no Docker do projeto, `cargo llvm-cov --workspace --all-targets --ignore-filename-regex 'main\.rs' --fail-under-lines 100`.
- Android (unitários e lint): `sh scripts/build-android.sh`, depois `cd android && ./gradlew testDebugUnitTest lintDebug assembleDebug` (os testes ficam em `android/app/src/test`, rodam na JVM; o lint ignora o código gerado em `uniffi/` por `android/app/lint.xml`; o `android.yml` roda os dois antes de montar o APK).
- Android (instrumentação, com emulador ou aparelho conectado): `cd android && ./gradlew connectedDebugAndroidTest`. Os testes (`android/app/src/androidTest`) usam a biblioteca nativa de verdade: o botão "Sugerir com IA" com um provedor falso (IA pronta, sem IA, notas internas só em textos internos, refinamentos) e o fluxo de documentos (importar, indexar em segundo plano, buscar, citar arquivo e página no pedido, estados de erro, remover sem deixar nada). Eles só mexem em documentos com o prefixo `teste-instrumentacao-` e os apagam ao final. **O CI não os roda** (exigiria um emulador); rode-os antes de uma release.
- No emulador, abra um chamado e use **Sugerir com IA** (cai no modelo pronto sem AICore); para exercitar a IA de verdade, use um aparelho com AICore e Gemini Nano.
