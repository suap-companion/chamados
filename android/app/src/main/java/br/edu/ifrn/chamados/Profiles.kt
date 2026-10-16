package br.edu.ifrn.chamados

import android.content.Context
import java.net.URI
import org.json.JSONArray
import org.json.JSONObject

/** Um perfil (ambiente) do app: o nome, o endereço do SUAP e a pasta da sessão e dos dados dele. */
data class AppProfile(val name: String, val url: String, val dir: String)

/**
 * Os perfis do app (por exemplo "IFRN" e "Local"): cada um tem o seu endereço do SUAP e a sua própria
 * sessão e dados (pasta própria). O que está em uso é o ativo; trocar de perfil não apaga nada.
 */
object Profiles {
    private const val PREFS = "chamados"
    private const val LIST = "profiles"
    private const val ACTIVE = "active_profile"

    private fun prefs(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    /** O nome que se dá, de início, ao perfil de um endereço. */
    fun nameFor(url: String): String = when (url) {
        PRODUCTION_URL -> "IFRN"
        LOCAL_URL -> "Local (emulador)"
        else -> runCatching { URI(url).authority }.getOrNull() ?: "SUAP"
    }

    /** Todos os perfis; o primeiro uso cria o perfil do endereço que já estava em uso (com a mesma pasta). */
    fun list(context: Context): List<AppProfile> {
        val stored = prefs(context).getString(LIST, null)
        if (stored != null) return parse(stored)
        val url = prefs(context).getString("base_url", PRODUCTION_URL) ?: PRODUCTION_URL
        val first = AppProfile(nameFor(url), url, CoreAccess.legacyDirName(url))
        save(context, listOf(first), first.name)
        return listOf(first)
    }

    fun active(context: Context): AppProfile {
        val all = list(context)
        val name = prefs(context).getString(ACTIVE, null)
        return all.firstOrNull { it.name == name } ?: all.first()
    }

    /** O endereço completo (com `/` no fim) e válido, ou `null`. */
    fun cleanUrl(text: String): String? {
        val trimmed = text.trim()
        val url = if (trimmed.endsWith("/")) trimmed else "$trimmed/"
        val valid = runCatching { URI(url) }.getOrNull()?.takeIf { (it.scheme == "https" || it.scheme == "http") && !it.host.isNullOrEmpty() }
        return valid?.let { url }
    }

    /** Cadastra um perfil novo e o torna o ativo; devolve o erro em texto, se houver. */
    fun add(context: Context, name: String, urlText: String): String? {
        val clean = name.trim()
        val url = cleanUrl(urlText) ?: return "Endereço inválido: use http:// ou https://"
        val all = list(context)
        if (clean.isEmpty()) return "Dê um nome ao perfil."
        if (all.any { it.name.equals(clean, ignoreCase = true) }) return "Já existe um perfil com esse nome."
        // A pasta é própria do perfil, mesmo que dois perfis apontem para o mesmo endereço.
        val taken = all.map { it.dir }.toSet()
        var dir = CoreAccess.legacyDirName(url)
        if (dir in taken) dir = "perfil-" + clean.lowercase().replace(Regex("[^a-z0-9]+"), "-").trim('-') + "-" + all.size
        save(context, all + AppProfile(clean, url, dir), clean)
        return null
    }

    fun activate(context: Context, name: String) {
        val all = list(context)
        if (all.any { it.name == name }) save(context, all, name)
    }

    /** Muda o endereço do perfil ativo (a pasta e a sessão continuam as dele). */
    fun changeUrl(context: Context, url: String) {
        val current = active(context)
        save(context, list(context).map { if (it.name == current.name) it.copy(url = url) else it }, current.name)
    }

    private fun save(context: Context, profiles: List<AppProfile>, active: String) {
        val json = JSONArray()
        profiles.forEach { json.put(JSONObject().put("name", it.name).put("url", it.url).put("dir", it.dir)) }
        prefs(context).edit().putString(LIST, json.toString()).putString(ACTIVE, active).apply()
    }

    private fun parse(text: String): List<AppProfile> {
        val array = JSONArray(text)
        return (0 until array.length()).map {
            val item = array.getJSONObject(it)
            AppProfile(item.getString("name"), item.getString("url"), item.getString("dir"))
        }
    }
}
