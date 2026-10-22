package br.edu.ifrn.chamados

import org.junit.Assert.assertEquals
import org.junit.Test

class FormatSizeTest {
    @Test
    fun `o tamanho vem em portugues`() {
        assertEquals("1,6 GB", formatSize(1_597_922_856L))
        assertEquals("546 MB", formatSize(546_660_344L))
        assertEquals("999 B", formatSize(999L))
    }
}
