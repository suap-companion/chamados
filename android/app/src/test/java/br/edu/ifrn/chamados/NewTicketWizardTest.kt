package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.chamados_mobile.CatalogInfo
import uniffi.chamados_mobile.PlaceItem

/** A lógica do assistente de "Novo chamado" (passos, recentes, campus e centros). */
class NewTicketWizardTest {
    private fun place(id: String, preferred: Boolean = false) = PlaceItem(id = id, name = "Lugar $id", preferred = preferred)

    @Test
    fun `o titulo e a barra contam os passos`() {
        assertEquals("Passo 1 de 3 · Serviço", stepTitle(WizardStep.Service))
        assertEquals("Passo 3 de 3 · Descrição", stepTitle(WizardStep.Description))
        assertEquals(1f / 3f, stepProgress(WizardStep.Service), 0.001f)
        assertEquals(1f, stepProgress(WizardStep.Description), 0.001f)
    }

    @Test
    fun `os recentes ficam sem repeticao e limitados`() {
        assertEquals(listOf("5"), withRecent(emptyList(), "5"))
        assertEquals(listOf("2", "1", "3"), withRecent(listOf("1", "2", "3"), "2"))
        val many = listOf("1", "2", "3", "4", "5")
        assertEquals(listOf("9", "1", "2", "3", "4"), withRecent(many, "9"))
        assertEquals(MAX_RECENTS, withRecent(many, "9").size)
    }

    @Test
    fun `os recentes guardados em texto ignoram o que nao e numero`() {
        assertEquals(listOf("400", "12", "7"), parseRecents("400, 12,7"))
        assertEquals(listOf("1"), parseRecents("1,,x,2a,-3"))
        assertTrue(parseRecents("").isEmpty())
    }

    @Test
    fun `o campus preferido vem escolhido, senao o primeiro`() {
        assertEquals("2", defaultPlace(listOf(place("1"), place("2", preferred = true)))?.id)
        assertEquals("1", defaultPlace(listOf(place("1"), place("2")))?.id)
        assertNull(defaultPlace(emptyList()))
    }

    @Test
    fun `varios centros pedem escolha e um so vale sozinho`() {
        assertFalse(centerRequired(emptyList()))
        assertFalse(centerRequired(listOf(place("5"))))
        assertTrue(centerRequired(listOf(place("5"), place("6"))))
    }

    @Test
    fun `o numero do servico precisa ser inteiro positivo`() {
        assertTrue(validServiceId(" 400 "))
        assertFalse(validServiceId(""))
        assertFalse(validServiceId("0"))
        assertFalse(validServiceId("-3"))
        assertFalse(validServiceId("abc"))
    }

    @Test
    fun `so continua com servico valido e centro escolhido quando ha varios`() {
        val two = listOf(place("5"), place("6"))
        assertFalse(canContinue(WizardStep.Service, "", emptyList(), null, ""))
        assertTrue(canContinue(WizardStep.Service, "400", listOf(place("5")), "5", ""))
        assertFalse(canContinue(WizardStep.Service, "400", two, null, ""))
        assertTrue(canContinue(WizardStep.Service, "400", two, "6", ""))
        assertTrue(canContinue(WizardStep.Interested, "", emptyList(), null, ""))
        assertFalse(canContinue(WizardStep.Description, "400", emptyList(), null, "  "))
        assertTrue(canContinue(WizardStep.Description, "400", emptyList(), null, "Preciso de ajuda"))
    }

    @Test
    fun `a linha do catalogo diz quantos servicos e quando foram lidos`() {
        assertTrue(catalogLine(null).contains("ainda não lido"))
        assertTrue(catalogLine(CatalogInfo(count = 0u, fetchedAt = null)).contains("ainda não lido"))
        val line = catalogLine(CatalogInfo(count = 120u, fetchedAt = 1_700_000_000_000uL))
        assertTrue(line.startsWith("120 serviços · lido em "))
    }
}

/** Os interessados recentes e a busca de pessoas do passo 2. */
class RecentPeopleTest {
    @Test
    fun `os recentes ficam sem repetir o vinculo e limitados`() {
        val ana = RecentPerson("1", "Ana")
        val bia = RecentPerson("2", "Bia")
        assertEquals(listOf(ana), withRecentPerson(emptyList(), ana))
        assertEquals(listOf(RecentPerson("2", "Bia nova"), ana), withRecentPerson(listOf(bia, ana), RecentPerson("2", "Bia nova")))
        val many = (1..5).map { RecentPerson("$it", "P$it") }
        val after = withRecentPerson(many, RecentPerson("9", "Nova"))
        assertEquals(MAX_RECENT_PEOPLE, after.size)
        assertEquals("9", after.first().id)
    }

    @Test
    fun `os recentes viram texto e voltam, ignorando linhas ruins`() {
        val people = listOf(RecentPerson("38349", "Wagner (Mat. 1723011)"), RecentPerson("7", "Ana\tSouza"))
        val text = encodeRecentPeople(people)
        assertEquals(listOf(RecentPerson("38349", "Wagner (Mat. 1723011)"), RecentPerson("7", "Ana Souza")), parseRecentPeople(text))
        assertTrue(parseRecentPeople("").isEmpty())
        assertEquals(listOf(RecentPerson("5", "Bia")), parseRecentPeople("x\tSem número\n5\tBia\n6\t \n8"))
    }

    @Test
    fun `a busca de pessoas pede ao menos dois caracteres`() {
        assertFalse(canSearchPeople(""))
        assertFalse(canSearchPeople(" a "))
        assertTrue(canSearchPeople("an"))
        assertTrue(canSearchPeople(" 1723011 "))
    }
}
