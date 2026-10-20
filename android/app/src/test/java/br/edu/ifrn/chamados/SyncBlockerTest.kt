package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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

    @Test
    fun `as opcoes manuais ficam recolhidas ate a pessoa escolher ou o QR deixar algo por fazer`() {
        // Aparelho novo: só o QR.
        assertFalse(showManualSetup(false, null))
        assertFalse(showManualSetup(false, state(configured = false, key = false, credentials = false)))
        // A pessoa escolheu não usar o QR.
        assertTrue(showManualSetup(true, null))
        // O QR sem credenciais trouxe bucket e chave: faltam as credenciais, então o campo aparece.
        assertTrue(showManualSetup(false, state(credentials = false)))
        // Tudo configurado (QR completo): nada manual.
        assertFalse(showManualSetup(false, state()))
    }

    @Test
    fun `o aviso de credenciais indica o QR que traz tudo`() {
        assertTrue(syncBlocker(state(credentials = false))!!.contains("--com-credenciais"))
    }
}
