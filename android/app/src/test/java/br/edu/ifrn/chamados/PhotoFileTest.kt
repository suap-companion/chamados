package br.edu.ifrn.chamados

import java.time.LocalDateTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A foto da câmera: nome, quanto reduzir e o que dizer quando a câmera não pode ser usada (lógica pura). */
class PhotoFileTest {
    @Test
    fun `o nome da foto leva a data e a hora`() {
        assertEquals("foto-20261023-141507.jpg", PhotoFile.fileName(LocalDateTime.of(2026, 10, 23, 14, 15, 7)))
    }

    @Test
    fun `cabendo no limite nao reduz e passando reduz por potencias de dois`() {
        val max = 9_500_000L
        assertEquals(1, PhotoFile.sampleSizeFor(3_000_000, max))
        assertEquals(1, PhotoFile.sampleSizeFor(max, max))
        // 2x o limite: escala linear de ~1,41, então metade da largura e da altura.
        assertEquals(2, PhotoFile.sampleSizeFor(max * 2, max))
        // 4x: escala 2; 5x: escala ~2,24, então 4.
        assertEquals(2, PhotoFile.sampleSizeFor(max * 4, max))
        assertEquals(4, PhotoFile.sampleSizeFor(max * 5, max))
        assertEquals(8, PhotoFile.sampleSizeFor(max * 40, max))
    }

    @Test
    fun `a orientacao da camera vira graus`() {
        assertEquals(90f, PhotoFile.rotationDegrees(android.media.ExifInterface.ORIENTATION_ROTATE_90))
        assertEquals(180f, PhotoFile.rotationDegrees(android.media.ExifInterface.ORIENTATION_ROTATE_180))
        assertEquals(270f, PhotoFile.rotationDegrees(android.media.ExifInterface.ORIENTATION_ROTATE_270))
        assertEquals(0f, PhotoFile.rotationDegrees(android.media.ExifInterface.ORIENTATION_NORMAL))
    }

    @Test
    fun `sem camera ou sem permissao o aviso oferece o arquivo, e com tudo certo nao ha aviso`() {
        assertTrue(cameraNote(hasCamera = false, permissionDenied = false)!!.contains("Escolher arquivos"))
        assertTrue(cameraNote(hasCamera = true, permissionDenied = true)!!.contains("permissão"))
        assertNotNull(cameraNote(hasCamera = false, permissionDenied = true))
        assertNull(cameraNote(hasCamera = true, permissionDenied = false))
    }
}
