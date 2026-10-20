// Central de ajuda do chamados: links de download e versão (release mais recente), busca e novidades.
// O HTML funciona sem este script: os links apontam para a página da release.
(function () {
  "use strict";
  var API = "https://api.github.com/repos/suap-companion/chamados/releases";
  var assets = {
    "android": "-android.apk",
    "windows": "-windows-x86_64.zip",
    "linux": "-linux-x86_64.tar.gz",
    "linux-arm": "-linux-arm64.tar.gz",
    "macos": "-macos-aarch64.tar.gz"
  };

  function json(url) {
    return fetch(url, { headers: { Accept: "application/vnd.github+json" } })
      .then(function (r) { return r.ok ? r.json() : Promise.reject(new Error("http " + r.status)); });
  }

  // ---- versão e downloads
  var versions = document.querySelectorAll("[data-version]");
  var links = document.querySelectorAll("[data-asset]");
  if (versions.length || links.length) {
    json(API + "/latest").then(function (release) {
      versions.forEach(function (el) { el.textContent = release.tag_name; });
      links.forEach(function (link) {
        var suffix = assets[link.getAttribute("data-asset")];
        (release.assets || []).forEach(function (asset) {
          if (suffix && asset.name.endsWith(suffix)) { link.href = asset.browser_download_url; }
        });
      });
    }).catch(function () {
      versions.forEach(function (el) { el.textContent = "mais recente"; });
    });
  }

  // ---- novidades por versão
  var list = document.getElementById("releases");
  if (list) {
    json(API + "?per_page=8").then(function (releases) {
      list.textContent = "";
      releases.forEach(function (release) {
        var item = document.createElement("li");
        var head = document.createElement("a");
        head.href = release.html_url;
        head.textContent = release.tag_name;
        var when = document.createElement("small");
        when.textContent = " · " + new Date(release.published_at).toLocaleDateString("pt-BR");
        var note = document.createElement("p");
        note.textContent = (release.body || "").split("\n")[0];
        item.appendChild(head);
        item.appendChild(when);
        item.appendChild(note);
        list.appendChild(item);
      });
    }).catch(function () {
      list.textContent = "";
      var item = document.createElement("li");
      var link = document.createElement("a");
      link.href = "https://github.com/suap-companion/chamados/releases";
      link.textContent = "Veja as novidades nas releases do GitHub";
      item.appendChild(link);
      list.appendChild(item);
    });
  }

  // ---- busca
  var index = [
    ["Instalar no Android", "instalar-android.html", "baixar apk instalar fontes desconhecidas entrar login gov.br atualizar assinatura"],
    ["Instalar o CLI", "instalar-cli.html", "cli terminal windows linux macos baixar path login senha perfil"],
    ["Ver e organizar a lista", "usar-app.html#lista", "lista abas meus suporte a fechar ativos adiados arquivados selos paginação"],
    ["Abrir um chamado", "usar-app.html#abrir", "novo chamado abrir serviço interessado descrição anexo"],
    ["Ler e atender um chamado", "usar-app.html#detalhe", "detalhe ações gestão mais comentar nota interna anexo resolver suspender cancelar reabrir linha do tempo abas"],
    ["Dar um título seu", "usar-app.html#titulo", "título local lápis editar"],
    ["Arquivar e adiar", "usar-app.html#arquivar-adiar", "arquivar adiar snooze voltar depois amanhã"],
    ["Selecionar vários chamados", "usar-app.html#lote", "lote selecionar vários tocar e segurar arquivar adiar comentar tags"],
    ["Receber avisos", "usar-app.html#avisos", "avisos notificações acompanhamento intervalo bateria"],
    ["Compartilhar do celular para um chamado", "usar-app.html#compartilhar", "compartilhar anexar texto arquivo galeria"],
    ["Trocar de conta ou de SUAP", "usar-app.html#perfis", "perfil conta sair suap local endereço"],
    ["Escrever com a ajuda da IA", "usar-app.html#ia", "ia inteligência artificial sugerir texto rascunho comentário resolução gemini nano mais curto formal"],
    ["A IA não aparece", "faq.html#ia", "ia sugerir aicore gemini nano modelo pronto preencher baixar"],
    ["Documentos de apoio", "usar-app.html#documentos", "documentos apoio pdf txt markdown importar indexar procedimento modelo citar fonte página"],
    ["Documento não indexado", "faq.html#documentos", "documento indexar sem texto ocr protegido senha pdf digitalizado grande"],
    ["Sincronizar com o computador", "usar-app.html#sincronizar", "sincronizar sync qr pareamento cifrada r2 s3"],
    ["Sincronizar no CLI e onde ficam os dados", "usar-cli.html#sincronizar", "sync sincronizar config.toml sync.key chave titles.json chaveiro bucket r2 s3 diretório onde ficam arquivos"],
    ["Usar o CLI", "usar-cli.html", "cli comandos list show open comment note resolve title archive snooze export watch"],
    ["Exportar para a IA", "usar-cli.html", "export zip chamado.md chamado.json ia análise"],
    ["Sessão expirada", "faq.html#sessao", "sessão expirada login entrar de novo"],
    ["Os avisos não chegam", "faq.html#avisos", "avisos não chegam atrasados bateria notificações"],
    ["Dúvidas e problemas", "faq.html", "faq dúvidas problemas anexo recusado notas internas oficial"],
    ["Privacidade e segurança", "privacidade.html", "privacidade senha dados cifrada segurança"],
    ["Fale conosco e novidades", "suporte.html", "contato relatar problema issue sugestão novidades versões"]
  ];
  var input = document.getElementById("busca");
  var results = document.getElementById("resultados");
  function norm(text) { return text.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, ""); }
  if (input && results) {
    input.addEventListener("input", function () {
      var terms = norm(input.value).split(/\s+/).filter(Boolean);
      results.textContent = "";
      if (!terms.length) { results.hidden = true; return; }
      var found = index.filter(function (entry) {
        var haystack = norm(entry[0] + " " + entry[2]);
        return terms.every(function (term) { return haystack.indexOf(term) >= 0; });
      }).slice(0, 6);
      if (!found.length) {
        var none = document.createElement("li");
        none.textContent = "Nada encontrado. Tente outras palavras.";
        results.appendChild(none);
      }
      found.forEach(function (entry) {
        var item = document.createElement("li");
        var link = document.createElement("a");
        link.href = entry[1];
        link.textContent = entry[0];
        item.appendChild(link);
        results.appendChild(item);
      });
      results.hidden = false;
    });
    document.addEventListener("click", function (event) {
      if (!results.contains(event.target) && event.target !== input) { results.hidden = true; }
    });
    input.addEventListener("keydown", function (event) { if (event.key === "Escape") { results.hidden = true; } });
  }
})();
