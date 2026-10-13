package br.edu.ifrn.chamados

import android.content.Context
import java.io.File
import java.net.URI
import uniffi.chamados_mobile.Chamados

/** Como as telas do app chegam à biblioteca Rust: o endereço escolhido e a pasta da sessão dele. */
object CoreAccess {
    private const val PREFS = "chamados"
    private const val BASE_URL = "base_url"

    fun baseUrl(context: Context): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(BASE_URL, PRODUCTION_URL)
            ?: PRODUCTION_URL

    fun saveBaseUrl(context: Context, url: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(BASE_URL, url).apply()
    }

    /** Cada endereço do SUAP tem a sua pasta, para as sessões não se misturarem. */
    fun dataDir(context: Context, url: String): File {
        val host = runCatching { URI(url).authority }.getOrNull() ?: "suap"
        val name = host.replace(Regex("[^A-Za-z0-9.-]"), "_")
        return File(context.filesDir, "perfil-$name")
    }

    /** Abre a sessão guardada do endereço atual (a de outra tela do app, no disco). */
    fun open(context: Context): Chamados {
        val url = baseUrl(context)
        val dir = dataDir(context, url).apply { mkdirs() }
        return Chamados(dir.absolutePath, url)
    }
}
