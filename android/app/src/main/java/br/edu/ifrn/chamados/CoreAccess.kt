package br.edu.ifrn.chamados

import android.content.Context
import java.io.File
import java.net.URI
import uniffi.chamados_mobile.Chamados

/** Como as telas do app chegam à biblioteca Rust: o perfil em uso, o endereço dele e a pasta da sessão. */
object CoreAccess {
    private const val PREFS = "chamados"

    /** O endereço do SUAP do perfil em uso. */
    fun baseUrl(context: Context): String = Profiles.active(context).url

    /** Muda o endereço do perfil em uso. */
    fun saveBaseUrl(context: Context, url: String) {
        Profiles.changeUrl(context, url)
    }

    /** O número do serviço usado da última vez ao abrir um chamado. */
    fun serviceId(context: Context): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString("service_id", "") ?: ""

    fun saveServiceId(context: Context, id: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString("service_id", id).apply()
    }

    /** O interessado (vínculo) usado da última vez ao abrir um chamado. */
    fun interested(context: Context): String =
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString("interested", "") ?: ""

    fun saveInterested(context: Context, id: String) {
        context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString("interested", id).apply()
    }

    /** O nome da pasta que as versões anteriores davam a um endereço (o primeiro perfil a mantém). */
    fun legacyDirName(url: String): String {
        val host = runCatching { URI(url).authority }.getOrNull() ?: "suap"
        return "perfil-" + host.replace(Regex("[^A-Za-z0-9.-]"), "_")
    }

    /** A pasta da sessão e dos dados de um perfil. */
    fun dataDir(context: Context, profile: AppProfile): File = File(context.filesDir, profile.dir)

    /** Abre a sessão guardada do perfil em uso (a de outra tela do app, no disco). */
    fun open(context: Context): Chamados {
        val profile = Profiles.active(context)
        val dir = dataDir(context, profile).apply { mkdirs() }
        return Chamados(dir.absolutePath, profile.url)
    }
}
