package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chamados_mobile.SyncState

/** O que falta para sincronizar, dito na tela (lógica pura). */
class SyncBlockerTest {
    private fun state(configured: Boolean = true, key: Boolean = true, credentials: Boolean = true) = SyncState(
        configured = configured, backend = "r2", endpoint = "https://x", bucket = "b", region = null, prefix = null,
        hasKey = key, hasCredentials = credentials,
    )

    @Test
    fun `sem estado ou sem bucket pede o QR ou o bucket`() {
        assertTrue(syncBlocker(null)!!.contains("bucket"))
        assertTrue(syncBlocker(state(configured = false))!!.contains("QR"))
    }

    @Test
    fun `com bucket e sem chave pede a chave`() {
        assertTrue(syncBlocker(state(key = false))!!.contains("chave"))
    }

    @Test
    fun `com bucket e chave e sem credenciais pede as credenciais e avisa que sincroniza sozinho`() {
        val message = syncBlocker(state(credentials = false))
        assertNotNull(message)
        assertTrue(message!!.contains("Access Key ID") && message.contains("sozinha"))
    }

    @Test
    fun `pronto para sincronizar nao tem bloqueio`() {
        assertNull(syncBlocker(state()))
    }

    @Test
    fun `a ordem do que falta e bucket, chave, credenciais`() {
        val noKeyNoCreds = syncBlocker(state(key = false, credentials = false))
        assertEquals(syncBlocker(state(key = false)), noKeyNoCreds)
    }
}
