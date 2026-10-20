package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chamados_mobile.AttachmentItem
import uniffi.chamados_mobile.EntryKind
import uniffi.chamados_mobile.ThreadItem
import uniffi.chamados_mobile.TicketView

/** As abas da linha do tempo: quais existem, o que mostram e em que ordem (lógica pura). */
class ThreadTabsTest {
    private fun item(kind: EntryKind, key: String, body: String) =
        ThreadItem(kind = kind, date = key, sortKey = key, author = null, photo = null, body = body)

    private fun view(canSeeNotes: Boolean = true) = TicketView(
        id = "7", title = "Chamado 7", heading = null, statuses = listOf("Aberto"), fields = emptyList(),
        thread = listOf(
            item(EntryKind.COMMENT, "2026-10-13 10:00:00", "novo"),
            item(EntryKind.EVENT, "2026-10-12 09:00:00", "aberto"),
        ),
        notes = listOf(item(EntryKind.NOTE, "2026-10-12 11:00:00", "nota")),
        canSeeNotes = canSeeNotes,
        attachments = listOf(AttachmentItem(number = 1u, name = "a.csv", by = null)),
        photo = null, url = "",
    )

    @Test
    fun `a aba de notas so existe para a equipe`() {
        assertTrue(ThreadTab.Notes in threadTabs(view(canSeeNotes = true)))
        assertFalse(ThreadTab.Notes in threadTabs(view(canSeeNotes = false)))
        assertEquals(4, threadTabs(view(true)).size)
    }

    @Test
    fun `tudo junta a linha do tempo e as notas do mais novo ao mais antigo`() {
        val bodies = entriesOf(view(), ThreadTab.All, newestFirst = true).map { it.body }
        assertEquals(listOf("novo", "nota", "aberto"), bodies)
    }

    @Test
    fun `a ordem escolhida inverte as entradas`() {
        val bodies = entriesOf(view(), ThreadTab.All, newestFirst = false).map { it.body }
        assertEquals(listOf("aberto", "nota", "novo"), bodies)
    }

    @Test
    fun `cada aba mostra so o seu tipo`() {
        assertEquals(listOf("novo"), entriesOf(view(), ThreadTab.Comments).map { it.body })
        assertEquals(listOf("nota"), entriesOf(view(), ThreadTab.Notes).map { it.body })
        assertTrue(entriesOf(view(), ThreadTab.Attachments).isEmpty())
    }

    @Test
    fun `as contagens seguem o conteudo`() {
        assertEquals(3, countOf(view(), ThreadTab.All))
        assertEquals(1, countOf(view(), ThreadTab.Comments))
        assertEquals(1, countOf(view(), ThreadTab.Notes))
        assertEquals(1, countOf(view(), ThreadTab.Attachments))
    }
}
