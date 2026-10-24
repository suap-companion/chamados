package br.edu.ifrn.chamados

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import java.text.DateFormat
import java.util.Date
import uniffi.chamados_mobile.AfterOpen
import uniffi.chamados_mobile.DraftKind
import uniffi.chamados_mobile.FieldValue
import uniffi.chamados_mobile.PlaceItem
import uniffi.chamados_mobile.ServiceFieldItem
import uniffi.chamados_mobile.TicketField
import uniffi.chamados_mobile.TicketView
import uniffi.chamados_mobile.ServiceItem

/** Os passos de "Novo chamado". */
enum class WizardStep(val label: String) { Service("Serviço"), Interested("Interessado"), Description("Descrição") }

/** "Passo 1 de 3 · Serviço". */
fun stepTitle(step: WizardStep): String = "Passo ${step.ordinal + 1} de ${WizardStep.entries.size} · ${step.label}"

/** A fração da barra de progresso: o passo atual conta como feito. */
fun stepProgress(step: WizardStep): Float = (step.ordinal + 1).toFloat() / WizardStep.entries.size

/** Quantos serviços usados por último se lembram. */
const val MAX_RECENTS = 5

/** Põe `id` no começo dos recentes, sem repetir, até [MAX_RECENTS]. */
fun withRecent(recents: List<String>, id: String): List<String> = (listOf(id) + recents.filter { it != id }).take(MAX_RECENTS)

/** Os recentes guardados como "400,12,7" (ignora o que não for número). */
fun parseRecents(text: String): List<String> = text.split(',').map { it.trim() }.filter { it.isNotEmpty() && it.all(Char::isDigit) }

/** O que vem escolhido numa lista de campus ou centros: o preferido do SUAP, ou o primeiro. */
fun defaultPlace(places: List<PlaceItem>): PlaceItem? = places.firstOrNull { it.preferred } ?: places.firstOrNull()

/**
 * Com mais de um centro de atendimento a escolha é obrigatória (como no `chamados open`); com um só, ele
 * vale sozinho e com nenhum não há como abrir.
 */
fun centerRequired(centers: List<PlaceItem>): Boolean = centers.size > 1

/** Uma pessoa escolhida como interessado: o número do vínculo e o nome para mostrar. */
data class RecentPerson(val id: String, val name: String)

/** Quantas pessoas se lembram como recentes. */
const val MAX_RECENT_PEOPLE = 5

/** Põe `person` no começo dos recentes, sem repetir o vínculo, até [MAX_RECENT_PEOPLE]. */
fun withRecentPerson(people: List<RecentPerson>, person: RecentPerson): List<RecentPerson> =
    (listOf(person) + people.filter { it.id != person.id }).take(MAX_RECENT_PEOPLE)

/** Os recentes guardados em texto, uma pessoa por linha ("vínculo<TAB>nome"); ignora linhas sem número ou sem nome. */
fun parseRecentPeople(text: String): List<RecentPerson> = text.lines().mapNotNull { line ->
    val (id, name) = line.split('\t', limit = 2).let { (it.getOrNull(0) ?: "") to (it.getOrNull(1) ?: "") }
    if (id.isNotEmpty() && id.all(Char::isDigit) && name.isNotBlank()) RecentPerson(id, name.trim()) else null
}

fun encodeRecentPeople(people: List<RecentPerson>): String =
    people.joinToString("\n") { "${it.id}\t${it.name.replace('\t', ' ').replace('\n', ' ')}" }

/** A busca de pessoas só começa com duas letras ou números, para não listar o SUAP inteiro. */
fun canSearchPeople(query: String): Boolean = query.trim().length >= 2

/** O texto do número do serviço é um número válido (o SUAP usa inteiros positivos). */
fun validServiceId(text: String): Boolean = text.trim().toLongOrNull()?.let { it > 0 } == true

/** Pode ir ao passo seguinte? Serviço: um número válido e o centro escolhido quando há vários. */
fun canContinue(
    step: WizardStep,
    serviceId: String,
    centers: List<PlaceItem>,
    center: String?,
    description: String,
    missingRequired: Boolean = false,
): Boolean =
    when (step) {
        WizardStep.Service -> validServiceId(serviceId) && (!centerRequired(centers) || center != null)
        WizardStep.Interested -> true
        WizardStep.Description -> description.isNotBlank() && !missingRequired
    }

/** O limite do título local (o mesmo do núcleo). */
const val MAX_TITLE = 120

/** Algum campo obrigatório do serviço está sem valor? */
fun missingRequiredField(fields: List<ServiceFieldItem>, values: Map<String, String>): Boolean =
    fields.any { it.required && (values[it.name] ?: it.value).isBlank() }

/** Os campos a mais com valor (o preenchido, ou o que o formulário já oferecia), para ir no `ShareJob`. */
fun fieldValues(fields: List<ServiceFieldItem>, values: Map<String, String>): List<FieldValue> =
    fields.mapNotNull { field -> (values[field.name] ?: field.value).trim().takeIf { it.isNotEmpty() }?.let { FieldValue(field.name, it) } }

/** O chamado que ainda não existe, só para dar contexto à IA que escreve a descrição (serviço e interessado). */
fun newTicketView(service: String?, interested: String?, title: String?): TicketView = TicketView(
    id = "", title = title?.takeIf { it.isNotBlank() } ?: "Novo chamado", heading = service, statuses = emptyList(),
    fields = listOfNotNull(interested?.let { TicketField("Interessado", it) }), thread = emptyList(), notes = emptyList(),
    canSeeNotes = false, attachments = emptyList(), photo = null, url = "",
)

/** O chamado foi aberto, mas algo depois falhou: a mensagem traz o número para a pessoa achá-lo. */
fun openedWithProblems(ticket: String, problems: List<String>): String =
    "O chamado #$ticket foi aberto, mas: " + problems.joinToString("; ")

/** Os rótulos do que fazer depois de abrir. */
fun AfterOpen.label(): String = when (this) {
    AfterOpen.NOTHING -> "Só abrir"
    AfterOpen.ASSUME -> "Assumir"
    AfterOpen.ASSUME_AND_START -> "Assumir e iniciar"
}

/** Quando o catálogo foi lido, para a pessoa saber se está velho. */
fun catalogLine(info: uniffi.chamados_mobile.CatalogInfo?): String {
    val read = info?.fetchedAt
    return when {
        info == null || read == null -> "Catálogo ainda não lido. Toque em Atualizar catálogo."
        else -> "${info.count} serviços · lido em ${DateFormat.getDateTimeInstance(DateFormat.SHORT, DateFormat.SHORT).format(Date(read.toLong()))}"
    }
}

/** O assistente de "Novo chamado": Serviço, Interessado e Descrição, com barra de progresso e Continuar no rodapé. */
@Composable
fun NewTicketScreen(model: MainViewModel) {
    BackHandler { model.wizardBack() }
    val step = model.wizardStep
    val ready = canContinue(step, model.serviceId, model.centers, model.center, model.draft, missingRequiredField(model.serviceFields, model.fieldValues))
    Scaffold(
        topBar = { GovBrHeader("Novo chamado", onBack = model::wizardBack) },
        bottomBar = {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                model.actionMessage?.let { Text(it, color = if (model.actionIsError) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary) }
                if (step == WizardStep.Description) {
                    Button(onClick = { model.createTicket(model.draft) }, enabled = ready && !model.actionBusy, modifier = Modifier.fillMaxWidth()) {
                        Text(if (model.actionBusy) "Enviando…" else "Abrir chamado")
                    }
                } else {
                    Button(onClick = model::wizardNext, enabled = ready, modifier = Modifier.fillMaxWidth()) { Text("Continuar") }
                }
            }
        },
    ) { padding ->
        Column(Modifier.padding(padding).fillMaxSize()) {
            Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(stepTitle(step), style = MaterialTheme.typography.labelLarge)
                LinearProgressIndicator(progress = { stepProgress(step) }, modifier = Modifier.fillMaxWidth())
            }
            Column(Modifier.padding(horizontal = 16.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                when (step) {
                    WizardStep.Service -> ServiceStep(model)
                    WizardStep.Interested -> InterestedStep(model)
                    WizardStep.Description -> DescriptionStep(model)
                }
            }
        }
    }
}

/** Uma linha de serviço escolhível: nome, e embaixo o grupo e o número; marca de incidente e de interno. */
@Composable
fun ServiceRow(name: String, detail: String, selected: Boolean, onClick: () -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick),
        colors = CardDefaults.cardColors(
            containerColor = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(name, style = MaterialTheme.typography.titleSmall)
            Text(detail, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

private fun ServiceItem.detail(): String =
    listOfNotNull(group.ifBlank { null }, "#$id", if (incident) "incidente" else null, if (internal) "interno" else null).joinToString(" · ")

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ServiceStep(model: MainViewModel) {
    LaunchedEffect(Unit) { model.loadCatalogInfo() }
    val defaultId = model.profileServiceId
    if (defaultId.isNotBlank()) {
        Text("Padrão do perfil", style = MaterialTheme.typography.titleSmall)
        val known = model.serviceName(defaultId)
        ServiceRow(known ?: "Serviço #$defaultId", "#$defaultId", model.serviceId.trim() == defaultId) { model.chooseService(defaultId) }
    }
    if (model.recents.isNotEmpty()) {
        Text("Recentes", style = MaterialTheme.typography.titleSmall)
        model.recents.filter { it != defaultId }.forEach { id ->
            ServiceRow(model.serviceName(id) ?: "Serviço #$id", "#$id", model.serviceId.trim() == id) { model.chooseService(id) }
        }
    }
    Text("Buscar serviço", style = MaterialTheme.typography.titleSmall)
    OutlinedTextField(
        value = model.serviceQuery,
        onValueChange = model::searchServices,
        label = { Text("Nome ou número do serviço") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    model.serviceResults.forEach { item ->
        ServiceRow(item.name, item.detail(), model.serviceId.trim() == item.id.toString()) { model.chooseService(item.id.toString(), item.name) }
    }
    if (model.serviceQuery.isNotBlank() && model.serviceResults.isEmpty() && validServiceId(model.serviceQuery)) {
        OutlinedButton(onClick = { model.chooseService(model.serviceQuery.trim()) }, modifier = Modifier.fillMaxWidth()) {
            Text("Usar o serviço #${model.serviceQuery.trim()}")
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(catalogLine(model.catalogInfo), style = MaterialTheme.typography.labelMedium, modifier = Modifier.weight(1f))
        TextButton(onClick = model::refreshCatalog, enabled = !model.catalogBusy) { Text(if (model.catalogBusy) "Lendo…" else "Atualizar catálogo") }
    }
    model.catalogMessage?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
    if (model.campuses.isNotEmpty()) {
        Text("Campus", style = MaterialTheme.typography.titleSmall)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            model.campuses.forEach { place ->
                FilterChip(selected = model.campus == place.id, onClick = { model.chooseCampus(place.id) }, label = { Text(place.name) })
            }
        }
    }
    if (model.centers.isNotEmpty()) {
        Text(if (centerRequired(model.centers)) "Centro de atendimento (escolha um)" else "Centro de atendimento", style = MaterialTheme.typography.titleSmall)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            model.centers.forEach { place ->
                FilterChip(selected = model.center == place.id, onClick = { model.chooseCenter(place.id) }, label = { Text(place.name) })
            }
        }
    }
    model.placesMessage?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
}

@Composable
private fun InterestedStep(model: MainViewModel) {
    val self = model.interested.isBlank()
    ServiceRow("Eu mesmo", model.user?.name ?: "A pessoa logada", selected = self) { model.chooseInterested(null) }
    if (model.recentPeople.isNotEmpty()) {
        Text("Recentes", style = MaterialTheme.typography.titleSmall)
        model.recentPeople.forEach { person ->
            ServiceRow(person.name, "vínculo ${person.id}", selected = model.interested.trim() == person.id) {
                model.chooseInterested(person)
            }
        }
    }
    Text("Buscar pessoa", style = MaterialTheme.typography.titleSmall)
    OutlinedTextField(
        value = model.personQuery,
        onValueChange = { model.personQuery = it },
        label = { Text("Matrícula ou nome") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        keyboardActions = KeyboardActions(onSearch = { model.searchPeople() }),
        modifier = Modifier.fillMaxWidth(),
    )
    OutlinedButton(onClick = model::searchPeople, enabled = canSearchPeople(model.personQuery) && !model.personBusy, modifier = Modifier.fillMaxWidth()) {
        Text(if (model.personBusy) "Buscando…" else "Buscar")
    }
    model.personMessage?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
    model.personResults.forEach { person ->
        ServiceRow(person.name, "vínculo ${person.id}", selected = model.interested.trim() == person.id) {
            model.chooseInterested(RecentPerson(person.id, person.name))
        }
    }
    val chosen = model.interested.trim()
    if (chosen.isNotEmpty() && model.recentPeople.none { it.id == chosen } && model.personResults.none { it.id == chosen }) {
        Text("Selecionado: ${model.interestedName ?: "vínculo $chosen"}", style = MaterialTheme.typography.labelLarge)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun DescriptionStep(model: MainViewModel) {
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
        if (uris.isNotEmpty()) model.onPicked(uris, 3u)
    }
    // O serviço e o interessado escolhidos: tocar volta ao passo.
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        AssistChip(onClick = { model.goToStep(WizardStep.Service) }, label = { Text(model.serviceName(model.serviceId.trim()) ?: "Serviço #${model.serviceId.trim()}") })
        AssistChip(onClick = { model.goToStep(WizardStep.Interested) }, label = { Text(model.interestedName ?: "Eu mesmo") })
    }
    OutlinedTextField(
        value = model.title,
        onValueChange = { model.title = it.take(MAX_TITLE) },
        label = { Text("Título local (opcional, só neste aparelho)") },
        supportingText = { Text("${model.title.length}/$MAX_TITLE") },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    OutlinedTextField(
        value = model.draft,
        onValueChange = { model.draft = it },
        label = { Text("Descrição") },
        minLines = 5,
        modifier = Modifier.fillMaxWidth(),
    )
    val view = remember(model.serviceId, model.interestedName, model.title) {
        newTicketView(model.serviceName(model.serviceId.trim()), model.interestedName, model.title)
    }
    AssistButton(DraftKind.DESCRIBE, view, model.title.ifBlank { null }, model.draft) { model.draft = it }
    model.serviceFields.forEach { field ->
        OutlinedTextField(
            value = model.fieldValues[field.name] ?: field.value,
            onValueChange = { model.setFieldValue(field.name, it) },
            label = { Text(if (field.required) "${field.label} *" else field.label) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
    }
    Text("Anexos (até 3)", style = MaterialTheme.typography.titleSmall)
    PhotoButton(model)
    OutlinedButton(onClick = { picker.launch("*/*") }, modifier = Modifier.fillMaxWidth()) { Text("Arquivo") }
    model.picked.forEach { Text("• ${it.name}") }
    if (model.picked.isNotEmpty()) TextButton(onClick = model::clearPicked) { Text("Limpar anexos") }
    model.pickNote?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
    Row(verticalAlignment = Alignment.CenterVertically) {
        Checkbox(checked = model.copyEmail, onCheckedChange = { model.copyEmail = it })
        Text("Enviar cópia por e-mail aos interessados")
    }
    Text("Depois de abrir", style = MaterialTheme.typography.titleSmall)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        AfterOpen.entries.forEach { option ->
            FilterChip(selected = model.afterOpen == option, onClick = { model.afterOpen = option }, label = { Text(option.label()) })
        }
    }
}
