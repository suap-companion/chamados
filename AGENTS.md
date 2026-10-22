# AGENTS.md

Guia para agentes (e pessoas) que trabalham neste repositório: o monorepo `suap-companion/chamados` (biblioteca Rust, CLI e app Android). Fale e escreva em **português do Brasil**.

## Mapa

- `crates/`: `chamados-core` (domínio, parsers), `chamados-app` (serviços sem terminal), `chamados-sync` (documento sincronizado e motor, sobre o `suap-sync`), `chamados-assist` (regras do "Sugerir com IA"), `chamados-mobile` (fachada UniFFI do app), `chamados-cli`.
- **Núcleo compartilhado**: `suap-core` (SUAP, sessão), `suap-rag` (documentos de apoio: SQLite/FTS5) e `suap-sync` (cripto, chaves, backends) vêm do repo [`suap-companion/suap-kit`](https://github.com/suap-companion/suap-kit) por dependência Git fixada em tag (`[workspace.dependencies]` do `Cargo.toml` da raiz). Não os copie para cá: mude o kit (PR, CI, tag) e atualize a tag. Testes de `suap-rag`/`suap-core`/`suap-sync` vivem lá (cobertura de 100% também).
- `android/`: app Kotlin + Compose (minSdk 26). As ligações Kotlin do Rust são **geradas** (`android/app/src/main/java/uniffi/`, ignoradas pelo git): `sh scripts/build-android.sh`.
- `site/`: central de ajuda (GitHub Pages, publicada pelo `pages.yml`). `docs/`: documentação técnica (`android.md`, `assistente.md`).
- O HTML real do SUAP local está em `../suap/centralservicos/templates`: consulte antes de adivinhar o markup de um parser.

## Validação (antes de entregar)

- **Rust** (sem Rust local: Docker `rust:latest`): `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo llvm-cov --workspace --all-targets --ignore-filename-regex 'main\.rs' --fail-under-lines 100`. Cobertura de **100% das linhas** é regra (RS-01): uma linha só com `)?;` conta como perdida (use constantes e chamadas curtas); closures e ramos nunca executados também.
- **Android**: `sh scripts/build-android.sh`, depois em `android/`: `./gradlew testDebugUnitTest lintDebug assembleDebug`.
- **Testes de instrumentação (emulador `chamados-spike` ou aparelho): `./gradlew connectedDebugAndroidTest`.** **O CI NÃO os roda** (exigiria um emulador). **Rode-os localmente antes de cada release e antes de abrir PR que mexa em `android/` ou em `crates/chamados-mobile`/`chamados-assist` ou na tag do `suap-rag`**, e registre o resultado no PR. Eles só tocam documentos com o prefixo `teste-instrumentacao-` e os apagam.
- Mudança em workflow do GitHub Actions: carregue a skill `github-actions` e consulte as versões das actions com `gh api repos/<dono>/<action>/releases/latest`.

## Regras do projeto

- **Privacidade**: nada de conteúdo de chamado, rascunho, senha, cookie ou chave de sincronização em logs, issues ou PRs. Notas internas nunca entram no contexto de textos públicos da IA (regra testada em `chamados-assist`).
- **Dependências novas** só com justificativa (custo no APK, manutenção, compatibilidade). O APK é pesado (ML Kit, SQLite); meça o tamanho.
- **Git**: branch `issues/<número>`; commits em pt-BR no padrão `tipo(escopo): [TAG] Descrição`, com **Motivo**, **Impacto** e **Arquivos** no corpo; autoria de Kelson da Costa Medeiros, sem trailer `Co-Authored-By`. Não commitar nem enviar sem o pedido da pessoa.
- **Release**: aumente a versão em `Cargo.toml` (`[workspace.package]`), faça o PR, espere o CI, mescle, crie a tag e a release (`gh release create vX.Y.Z`: o workflow Release só dispara em `release: published`), espere o workflow **terminar com sucesso (5 assets, inclusive o APK)** e só então feche a issue. Registre as horas (`Estimated (h)`/`Spent (h)`) nas issues.
- Scripts Python com acentos: grave o `.py` num arquivo (não por heredoc/stdin; o Python do Windows lê stdin em cp1252) e rode com `python -I`.

## Limitações conhecidas

- A IA do sistema (Gemini Nano) só foi exercitada até onde o emulador permite (o modelo local foi removido, #165); veja a issue de teste em aparelho real (#142) e `docs/assistente.md`.
- A busca dos documentos é por palavras (FTS5/BM25), sem sinônimos.
