package br.edu.ifrn.chamados

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.net.URI
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import uniffi.chamados_mobile.cloudGenerate

/** Tudo o que a IA na nuvem precisa para um pedido: o provedor, o modelo, o endereço e a chave da pessoa. */
data class CloudConfig(val provider: String, val model: String?, val baseUrl: String?, val key: String)

/** A chave para a tela: só o fim, nunca inteira ("••••1234"). */
fun maskKey(key: String): String = "••••" + key.trim().takeLast(4)

/** O servidor que receberá o texto, para o aviso (o endereço informado ou o padrão do provedor). */
fun hostOf(address: String?, fallback: String): String =
    runCatching { URI(address?.trim().takeUnless { it.isNullOrEmpty() } ?: fallback).host }.getOrNull() ?: fallback

/**
 * O cofre da chave de API: o arquivo fica cifrado (AES-GCM) com uma chave do Android Keystore, que não sai do
 * aparelho, e na pasta que o backup do Android não leva. Ler uma chave que não abre (outro aparelho, chave do
 * Keystore apagada) devolve `null`, como se não houvesse chave.
 */
class KeyVault(private val dir: File, private val alias: String = "chamados_ia_cofre") {
    private fun secretKey(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private fun file(name: String) = File(dir, name.filter { it.isLetterOrDigit() } + ".bin")

    fun has(name: String): Boolean = file(name).isFile

    fun save(name: String, secret: String) {
        val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, secretKey()) }
        dir.mkdirs()
        file(name).writeBytes(cipher.iv + cipher.doFinal(secret.toByteArray(Charsets.UTF_8)))
    }

    fun load(name: String): String? = runCatching {
        val bytes = file(name).readBytes()
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(TAG_BITS, bytes.copyOfRange(0, IV_BYTES)))
        String(cipher.doFinal(bytes.copyOfRange(IV_BYTES, bytes.size)), Charsets.UTF_8)
    }.getOrNull()

    fun clear(name: String) {
        file(name).delete()
    }

    private companion object {
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
        const val TAG_BITS = 128
    }
}

/** As escolhas da pessoa para a IA na nuvem (opcional, desligada até ela ler o aviso e ligar). */
object CloudAi {
    private fun prefs(context: Context) = context.getSharedPreferences("chamados", Context.MODE_PRIVATE)

    fun vault(context: Context) = KeyVault(File(context.noBackupFilesDir, "cofre"))

    fun enabled(context: Context): Boolean = prefs(context).getBoolean("cloud_enabled", false)

    fun setEnabled(context: Context, value: Boolean) {
        prefs(context).edit().putBoolean("cloud_enabled", value).apply()
    }

    fun provider(context: Context): String = prefs(context).getString("cloud_provider", null) ?: "gemini"

    fun model(context: Context, provider: String): String? = prefs(context).getString("cloud_model_$provider", null)

    fun baseUrl(context: Context, provider: String): String? = prefs(context).getString("cloud_url_$provider", null)

    fun save(context: Context, provider: String, model: String, baseUrl: String) {
        prefs(context).edit()
            .putString("cloud_provider", provider)
            .putString("cloud_model_$provider", model.trim())
            .putString("cloud_url_$provider", baseUrl.trim())
            .apply()
    }

    /** O que o pedido usa agora, ou `null` se está desligada ou sem chave. */
    fun config(context: Context): CloudConfig? {
        if (!enabled(context)) return null
        val provider = provider(context)
        val key = vault(context).load(provider) ?: return null
        return CloudConfig(provider, model(context, provider)?.ifBlank { null }, baseUrl(context, provider)?.ifBlank { null }, key)
    }
}

/**
 * A IA na nuvem como provedor: pronta se está ligada e há chave. O Rust (`cloudGenerate`) fala com o provedor e
 * devolve o texto de uma vez; a limpeza e as regras de privacidade do pedido são as mesmas de sempre.
 */
class CloudAnswerProvider(
    private val config: () -> CloudConfig?,
    private val ask: suspend (CloudConfig, String) -> String = { cloud, prompt ->
        cloudGenerate(cloud.provider, cloud.model, cloud.baseUrl, cloud.key, prompt)
    },
) : AnswerProvider {
    override suspend fun state(): AiState = if (config() != null) AiState.Ready else AiState.Unsupported

    override fun download(): Flow<Long> = emptyFlow()

    override fun generate(prompt: String): Flow<String> = flow {
        val cloud = checkNotNull(config()) { "A IA na nuvem está desligada ou sem chave" }
        emit(ask(cloud, prompt))
    }
}
