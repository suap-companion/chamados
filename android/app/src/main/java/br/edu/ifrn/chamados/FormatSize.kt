package br.edu.ifrn.chamados

import java.util.Locale

/** Tamanho legível em português: "1,6 GB", "546 MB". */
fun formatSize(bytes: Long): String = when {
    bytes >= 1_000_000_000L -> String.format(Locale.forLanguageTag("pt-BR"), "%.1f GB", bytes / 1e9)
    bytes >= 1_000_000L -> "${bytes / 1_000_000} MB"
    else -> "$bytes B"
}
