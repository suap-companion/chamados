package br.edu.ifrn.chamados

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

/** O modelo local: tamanhos, espaço, SHA-256 e a instalação só do arquivo que confere (sem Android). */
class LocalModelTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"

    private fun spec(size: Long = 3, sha: String = abc) = ModelSpec("m.litertlm", size, sha)

    @Test
    fun `o tamanho vem em portugues`() {
        assertEquals("1,6 GB", formatSize(1_597_922_856L))
        assertEquals("546 MB", formatSize(546_660_344L))
        assertEquals("999 B", formatSize(999L))
    }

    @Test
    fun `o espaco pede vinte por cento de folga`() {
        assertTrue(enoughSpace(freeBytes = 120, neededBytes = 100))
        assertFalse(enoughSpace(freeBytes = 119, neededBytes = 100))
        assertFalse(enoughSpace(freeBytes = 0, neededBytes = 1))
    }

    @Test
    fun `o sha256 bate com o vetor conhecido`() {
        val file = folder.newFile("abc.txt").apply { writeText("abc") }
        assertEquals(abc, Hashing.sha256(file))
    }

    @Test
    fun `o modelo publicado tem o tamanho e o hash esperados`() {
        assertEquals(64, LocalModel.SPEC.sha256.length)
        assertTrue(LocalModel.SPEC.sha256.all { it in "0123456789abcdef" })
        assertTrue(LocalModel.URL.endsWith(LocalModel.FILE))
        assertTrue(LocalModel.URL.startsWith("https://"))
    }

    @Test
    fun `so o arquivo que confere vira modelo instalado`() {
        val files = LocalModelFiles(folder.root, spec())
        assertFalse(files.isInstalled())
        assertFalse(files.verifyAndInstall()) // nada baixado
        files.partial.writeText("abc")
        assertTrue(files.verifyAndInstall())
        assertTrue(files.isInstalled())
        assertFalse(files.partial.exists())
        assertEquals("abc", files.model.readText())
    }

    @Test
    fun `um arquivo com hash ou tamanho errado e apagado e nao instala`() {
        val wrongHash = LocalModelFiles(folder.root, spec(sha = "0".repeat(64)))
        wrongHash.partial.writeText("abc")
        assertFalse(wrongHash.verifyAndInstall())
        assertFalse(wrongHash.partial.exists())
        assertFalse(wrongHash.isInstalled())

        val wrongSize = LocalModelFiles(folder.root, spec(size = 4))
        wrongSize.partial.writeText("abc")
        assertFalse(wrongSize.verifyAndInstall(sha = { abc }))
        assertFalse(wrongSize.partial.exists())
    }

    @Test
    fun `apagar tira o modelo e o que sobrou do download`() {
        val files = LocalModelFiles(File(folder.root, "models").also { it.mkdirs() }, spec())
        files.model.writeText("abc")
        files.partial.writeText("ab")
        assertTrue(files.isInstalled())
        files.deleteAll()
        assertFalse(files.model.exists())
        assertFalse(files.partial.exists())
    }

    @Test
    fun `a instalacao substitui um modelo antigo`() {
        val files = LocalModelFiles(folder.root, spec())
        files.model.writeText("velho")
        files.partial.writeText("abc")
        assertTrue(files.verifyAndInstall())
        assertEquals("abc", files.model.readText())
    }

    @Test
    fun `um pedaco que repete o acumulado nao duplica o texto`() {
        assertEquals("Olá, tudo", merge("Olá,", " tudo"))
        assertEquals("Olá, tudo bem", merge("Olá, tudo", "Olá, tudo bem"))
        assertEquals("abc", merge("", "abc"))
        assertEquals("aa", merge("a", "a")) // mesmo tamanho: é um pedaço novo
    }
}
