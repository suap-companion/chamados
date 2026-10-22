package br.edu.ifrn.chamados

import android.app.Application
import android.net.Uri
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.core.app.ApplicationProvider
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import uniffi.chamados_mobile.DocState
import uniffi.chamados_mobile.DocInfo
import uniffi.chamados_mobile.TicketView
import uniffi.chamados_mobile.textToPdf

/**
 * O fluxo dos documentos de apoio no aparelho: importar (como a tela faz), indexar em segundo plano pelo
 * WorkManager, buscar, usar no pedido da IA e remover sem deixar nada. Os documentos de teste têm nome próprio
 * e são removidos ao final: os da pessoa não são tocados.
 */
class DocumentsFlowTest {
    @get:Rule
    val compose = createComposeRule()

    private val app: Application = ApplicationProvider.getApplicationContext()
    private val prefix = "teste-instrumentacao-"
    private lateinit var model: DocumentsViewModel

    @Before
    fun setUp() {
        InstrumentationRegistry.getInstrumentation().runOnMainSync { model = DocumentsViewModel(app) }
    }

    @After
    fun cleanUp() {
        val docs = Documents.open(app)
        docs.list().filter { it.name.startsWith(prefix) }.forEach {
            Documents.cancel(app, it.id)
            docs.remove(it.id)
        }
    }

    private fun file(name: String, bytes: ByteArray): Uri {
        val file = File(app.cacheDir, name).apply { writeBytes(bytes) }
        return Uri.fromFile(file)
    }

    private fun mine(): List<DocInfo> = Documents.open(app).list().filter { it.name.startsWith(prefix) }

    private fun waitFor(timeoutMs: Long = 30_000, condition: () -> Boolean) {
        val end = System.currentTimeMillis() + timeoutMs
        while (!condition()) {
            check(System.currentTimeMillis() < end) { "tempo esgotado esperando a indexação" }
            Thread.sleep(200)
        }
    }

    private fun importAndIndex(name: String, bytes: ByteArray): DocInfo {
        InstrumentationRegistry.getInstrumentation().runOnMainSync { model.import(listOf(file(name, bytes))) }
        waitFor { mine().any { it.name == name } }
        // O WorkManager indexa em segundo plano; acompanha pelo banco, como a tela.
        waitFor { mine().first { it.name == name }.state == DocState.READY }
        return mine().first { it.name == name }
    }

    @Test
    fun a_text_document_is_imported_indexed_in_the_background_found_and_removed() {
        val name = prefix + "vpn.txt"
        val text = "Para acessar a rede do campus de casa, instale o cliente VPN e use o usuário do SUAP."
        val document = importAndIndex(name, text.toByteArray())
        assertTrue(document.chunks >= 1u)

        val hits = Documents.open(app).search("como acessar a VPN do campus", 3u)
        assertTrue(hits.any { it.source == name && it.text.contains("cliente VPN") })

        val before = Documents.dir(app).walkTopDown().filter { it.isFile && it.name.endsWith(name) }.count()
        assertEquals(1, before)
        InstrumentationRegistry.getInstrumentation().runOnMainSync { model.remove(document.id) }
        waitFor { mine().none { it.name == name } }
        // Nem o arquivo copiado, nem os trechos achados.
        assertEquals(0, Documents.dir(app).walkTopDown().filter { it.isFile && it.name.endsWith(name) }.count())
        assertTrue(Documents.open(app).search("cliente VPN campus", 3u).none { it.source == name })
    }

    @Test
    fun a_pdf_is_indexed_by_page_and_its_snippets_feed_the_draft_with_file_and_page() {
        val name = prefix + "manual.pdf"
        val pdf = textToPdf(null, "Para reiniciar a impressora, desligue o aparelho da tomada por trinta segundos.")
        importAndIndex(name, pdf)

        val ticket = TicketView(
            id = "9", title = "Chamado 9", heading = "Impressora", statuses = listOf("Aberto"), fields = emptyList(),
            thread = emptyList(), notes = emptyList(), canSeeNotes = false, attachments = emptyList(), photo = null, url = "",
        )
        val references = kotlinx.coroutines.runBlocking { Documents.references(app, ticket, "como reiniciar a impressora", null) }
        assertNotNull(references.firstOrNull { it.source == name })
        val first = references.first { it.source == name }
        assertEquals(1u, first.page)
        assertTrue(first.text.contains("tomada"))

        val prompt = uniffi.chamados_mobile.assistPrompt(
            uniffi.chamados_mobile.DraftInput(
                kind = uniffi.chamados_mobile.DraftKind.RESOLVE, ticket = ticket, localTitle = null, hint = null,
                previous = null, refine = null, references = references,
            ),
            null,
        )
        assertTrue(prompt.text.contains("Documentos de apoio"))
        assertTrue(prompt.text.contains("$name, p. 1"))
    }

    @Test
    fun problem_files_get_a_clear_state_and_unsupported_ones_are_refused() {
        val fake = prefix + "falso.pdf"
        InstrumentationRegistry.getInstrumentation().runOnMainSync { model.import(listOf(file(fake, "isto não é um pdf".toByteArray()))) }
        waitFor { mine().any { it.name == fake && it.state == DocState.CORRUPTED } }
        assertEquals("Arquivo inválido ou danificado.", mine().first { it.name == fake }.statusText())

        // Um PDF de verdade, protegido por senha (`androidTest/assets/protegido.pdf`, gerado por
        // `scripts/gerar-pdf-protegido.py`): fica "protegido" e o texto dele nunca entra na busca.
        val locked = prefix + "protegido.pdf"
        val bytes = InstrumentationRegistry.getInstrumentation().context.assets.open("protegido.pdf").use { it.readBytes() }
        InstrumentationRegistry.getInstrumentation().runOnMainSync { model.import(listOf(file(locked, bytes))) }
        waitFor { mine().any { it.name == locked && it.state == DocState.PROTECTED } }
        assertTrue(mine().first { it.name == locked }.statusText().contains("senha"))
        assertTrue(Documents.open(app).search("Documento protegido por senha", 3u).none { it.source == locked })

        val sheet = prefix + "planilha.xlsx"
        InstrumentationRegistry.getInstrumentation().runOnMainSync { model.import(listOf(file(sheet, ByteArray(10)))) }
        waitFor { model.message != null }
        assertTrue(model.message!!.contains("use PDF, TXT ou Markdown"))
        assertFalse(mine().any { it.name == sheet })
    }
}
