package br.edu.ifrn.chamados

import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import java.time.format.DateTimeParseException
import uniffi.chamados_mobile.TicketView

/**
 * O que o cabeçalho do detalhe mostra além do título e da situação, tirado dos campos da página do chamado.
 * Os campos que o SUAP não informa ficam `null`.
 */
data class HeaderFacts(
    /** O serviço (o assunto do chamado) e o campus do centro de atendimento: "Serviço · campus". */
    val service: String?,
    val campus: String?,
    val interested: String?,
    /** O atendente atual; `null` = ninguém assumiu. */
    val owner: String?,
    /** A data limite do SLA como o SUAP a escreve (sem o "Faltam …"), se informada. */
    val deadline: String?,
    /** O prazo do SLA já passou. */
    val overdue: Boolean,
)

private val DEADLINE_FORMATS = listOf(
    DateTimeFormatter.ofPattern("dd/MM/yyyy HH:mm:ss"),
    DateTimeFormatter.ofPattern("dd/MM/yyyy HH:mm"),
)

/** A data e a hora no começo do texto ("23/10/2026 17:00:00 (Faltam 2 horas)"), ou `null` se não estiver nesse formato. */
fun parseDeadline(text: String): LocalDateTime? {
    val start = Regex("""\d{2}/\d{2}/\d{4}[ ,]+(?:às\s+)?\d{2}:\d{2}(?::\d{2})?""").find(text)?.value ?: return null
    val clean = start.replace(Regex("""[ ,]+(?:às\s+)?"""), " ")
    for (format in DEADLINE_FORMATS) {
        try {
            return LocalDateTime.parse(clean, format)
        } catch (_: DateTimeParseException) {
            // tenta o próximo formato
        }
    }
    return null
}

private fun TicketView.field(vararg names: String): String? =
    fields.firstOrNull { it.label.trim().trimEnd(':').lowercase() in names }?.value?.trim()?.takeIf { it.isNotEmpty() }

/** Os fatos do cabeçalho de um chamado; `now` decide se o prazo do SLA passou. */
fun headerFacts(view: TicketView, now: LocalDateTime): HeaderFacts {
    // O cabeçalho do SUAP é "Área | Serviço": o serviço é a última parte.
    val service = (view.heading ?: view.title).substringAfterLast(" | ").trim().ifEmpty { null }
    // "Centro de atendimento": "Nome do centro /SIGLA" quando o chamado tem campus.
    val center = view.field("centro de atendimento")
    val campus = center?.let { Regex("""\s/\s*([A-Za-z0-9]{2,8})\s*$""").find(it)?.groupValues?.get(1) }
    val raw = view.field("data limite")
    val deadlineAt = raw?.let(::parseDeadline)
    val deadline = raw?.substringBefore("(")?.trim()?.ifEmpty { null }
    // "Faltam …" só aparece enquanto o prazo não passou; sem ele, vale a data.
    val overdue = raw != null && !raw.contains("Faltam", ignoreCase = true) && deadlineAt != null && deadlineAt.isBefore(now)
    return HeaderFacts(
        service = service,
        campus = campus,
        interested = view.field("interessado"),
        owner = view.field("atendente"),
        deadline = deadline,
        overdue = overdue,
    )
}

/** "Serviço · campus" (só o que existe), ou `null` se não há nenhum dos dois. */
fun HeaderFacts.serviceLine(): String? = listOfNotNull(service, campus).joinToString(" · ").ifEmpty { null }
