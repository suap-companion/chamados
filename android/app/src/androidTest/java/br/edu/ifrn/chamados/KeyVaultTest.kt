package br.edu.ifrn.chamados

import androidx.test.core.app.ApplicationProvider
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** O cofre da chave de API no aparelho: cifra de verdade com o Keystore, devolve igual e não deixa a chave em claro. */
class KeyVaultTest {
    private val dir = File(ApplicationProvider.getApplicationContext<android.app.Application>().cacheDir, "cofre-teste")
    private val vault = KeyVault(dir, alias = "chamados_ia_cofre_teste")

    @After
    fun cleanUp() {
        dir.deleteRecursively()
        runCatching {
            java.security.KeyStore.getInstance("AndroidKeyStore").apply { load(null) }.deleteEntry("chamados_ia_cofre_teste")
        }
    }

    @Test
    fun a_saved_key_comes_back_the_same_and_is_never_on_disk_in_the_clear() {
        assertFalse(vault.has("openai"))
        assertNull(vault.load("openai"))

        vault.save("openai", "sk-segredo-1234")
        assertTrue(vault.has("openai"))
        assertEquals("sk-segredo-1234", vault.load("openai"))
        val onDisk = dir.listFiles().orEmpty().joinToString("") { String(it.readBytes(), Charsets.ISO_8859_1) }
        assertFalse(onDisk.contains("segredo"))

        // Cada provedor tem a sua; trocar a chave substitui, e apagar leva embora.
        vault.save("gemini", "outra")
        vault.save("openai", "sk-nova")
        assertEquals("sk-nova", vault.load("openai"))
        assertEquals("outra", vault.load("gemini"))
        vault.clear("openai")
        assertNull(vault.load("openai"))
        assertEquals("outra", vault.load("gemini"))
    }

    @Test
    fun a_damaged_or_foreign_file_reads_as_no_key() {
        vault.save("openai", "sk-segredo-1234")
        File(dir, "openai.bin").writeBytes(ByteArray(40) { it.toByte() })
        assertNull(vault.load("openai"))
    }
}
