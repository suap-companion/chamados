package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chamados_mobile.DocInfo
import uniffi.chamados_mobile.DocState

/** O que a tela de documentos mostra: tipos aceitos e a frase de cada estado (lógica pura). */
class DocumentsTest {
    private fun doc(state: DocState, pages: UInt? = null, indexed: UInt = 0u, chunks: UInt = 0u) =
        DocInfo(id = 1, name = "a.pdf", kind = "pdf", size = 10u, state = state, pages = pages, indexedPages = indexed, chunks = chunks)

    @Test
    fun `so pdf txt e markdown sao aceitos sem diferenciar maiusculas`() {
        listOf("a.pdf", "A.PDF", "notas.txt", "x.text", "LEIA.md", "guia.markdown").forEach { assertTrue(it, isSupportedDocument(it)) }
        listOf("planilha.xlsx", "foto.png", "semextensao", "pdf").forEach { assertFalse(it, isSupportedDocument(it)) }
    }

    @Test
    fun `na fila e indexando precisam de indexacao`() {
        assertTrue(doc(DocState.PENDING).needsIndexing())
        assertTrue(doc(DocState.INDEXING).needsIndexing())
        DocState.entries.filter { it != DocState.PENDING && it != DocState.INDEXING }.forEach {
            assertFalse(it.name, doc(it).needsIndexing())
        }
    }

    @Test
    fun `cada estado tem a sua frase`() {
        assertEquals("Na fila para indexar.", doc(DocState.PENDING).statusText())
        assertEquals("Parado em 2 de 7 páginas; continua sozinho.", doc(DocState.PENDING, pages = 7u, indexed = 2u).statusText())
        assertEquals("Indexando…", doc(DocState.INDEXING).statusText())
        assertEquals("Indexando… página 3 de 7", doc(DocState.INDEXING, pages = 7u, indexed = 3u).statusText())
        assertEquals("Pronto: 12 trechos.", doc(DocState.READY, chunks = 12u).statusText())
        assertTrue(doc(DocState.NO_TEXT).statusText().contains("OCR"))
        assertTrue(doc(DocState.PROTECTED).statusText().contains("senha"))
        assertEquals("Arquivo inválido ou danificado.", doc(DocState.CORRUPTED).statusText())
        assertTrue(doc(DocState.TOO_LARGE).statusText().contains("50 MB"))
    }
}
