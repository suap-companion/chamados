package br.edu.ifrn.chamados

import com.google.mlkit.genai.common.GenAiException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chamados_mobile.DraftKind

/** O que cada ação escreve e as mensagens de falha da IA (lógica pura, sem a biblioteca nativa). */
class AssistantTest {
    @Test
    fun `cada acao com texto tem o seu tipo de rascunho`() {
        assertEquals(DraftKind.COMMENT, DetailAction.Comment.draftKind())
        assertEquals(DraftKind.INTERNAL_NOTE, DetailAction.Note.draftKind())
        assertEquals(DraftKind.RESOLVE, DetailAction.Resolve.draftKind())
        assertEquals(DraftKind.SUSPEND, DetailAction.Suspend.draftKind())
        assertEquals(DraftKind.CANCEL, DetailAction.Cancel.draftKind())
        assertEquals(DraftKind.REOPEN, DetailAction.Reopen.draftKind())
        assertEquals(DraftKind.CLOSE, DetailAction.Close.draftKind())
    }

    @Test
    fun `acoes sem texto nao tem rascunho`() {
        assertNull(DetailAction.Assume.draftKind())
        assertNull(DetailAction.Start.draftKind())
        assertNull(DetailAction.Attach.draftKind())
    }

    @Test
    fun `so escalar e devolver escrevem uma nota na gestao`() {
        assertEquals(DraftKind.ESCALATE, ManageAction.Escalate.draftKind())
        assertEquals(DraftKind.RETURN, ManageAction.Return.draftKind())
        val others = ManageAction.entries.filter { it != ManageAction.Escalate && it != ManageAction.Return }
        assertTrue(others.all { it.draftKind() == null })
    }

    @Test
    fun `as falhas da IA viram mensagens em portugues sem o texto do chamado`() {
        val binding = GenAiException(IllegalStateException("segredo do chamado"), 601).assistMessage()
        val preparing = GenAiException(IllegalStateException("segredo do chamado"), 606).assistMessage()
        val other = GenAiException(IllegalStateException("segredo do chamado"), 12).assistMessage()
        val generic = IllegalStateException("segredo do chamado").assistMessage()
        assertTrue(binding.contains("AICore"))
        assertTrue(preparing.contains("preparada"))
        assertTrue(other.contains("código 12"))
        assertEquals("Não foi possível gerar o texto agora. Tente de novo.", generic)
        listOf(binding, preparing, other, generic).forEach { assertFalse(it.contains("segredo")) }
    }
}
