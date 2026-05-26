package dev.quartermaster.android

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import dev.quartermaster.android.generated.models.PantrySuggestionDto
import dev.quartermaster.android.generated.models.RecipeDto
import dev.quartermaster.android.generated.models.RecipeExecutionPreflightResponse
import dev.quartermaster.android.generated.models.RecipeExecutionResponse
import dev.quartermaster.android.generated.models.RecipeSummaryDto
import dev.quartermaster.android.generated.models.SupplierCartDraftDto
import dev.quartermaster.android.generated.models.SupplierOrderDto
import kotlinx.coroutines.launch

private enum class CookSection {
    Recipes,
    Suggestions,
    Shopping,
}

private val CookSection.label: String
    get() = when (this) {
        CookSection.Recipes -> "Recipes"
        CookSection.Suggestions -> "Suggestions"
        CookSection.Shopping -> "Shopping"
    }

@Composable
internal fun CookScreen(
    appState: QuartermasterAppState,
    modifier: Modifier = Modifier,
    onBack: () -> Unit = {},
) {
    val scope = rememberCoroutineScope()
    var section by remember { mutableStateOf(CookSection.Recipes) }
    var recipes by remember { mutableStateOf<List<RecipeSummaryDto>>(emptyList()) }
    var suggestions by remember { mutableStateOf<List<PantrySuggestionDto>>(emptyList()) }
    var suggestionWarnings by remember { mutableStateOf<List<String>>(emptyList()) }
    var selectedRecipe by remember { mutableStateOf<RecipeDto?>(null) }
    var preflight by remember { mutableStateOf<RecipeExecutionPreflightResponse?>(null) }
    var execution by remember { mutableStateOf<RecipeExecutionResponse?>(null) }
    var allowPartial by remember { mutableStateOf(false) }
    var cartDraft by remember { mutableStateOf<SupplierCartDraftDto?>(null) }
    var order by remember { mutableStateOf<SupplierOrderDto?>(null) }
    var localError by remember { mutableStateOf<String?>(null) }

    fun launchCook(block: suspend () -> Unit) {
        scope.launch {
            localError = null
            runCatching { block() }
                .onFailure { localError = it.message ?: "Action failed" }
        }
    }

    LaunchedEffect(appState.currentHouseholdId) {
        recipes = runCatching { appState.loadCookRecipes() }.getOrDefault(emptyList())
        suggestions = runCatching { appState.loadPantrySuggestions() }.getOrDefault(emptyList())
    }

    LazyColumn(
        modifier = modifier
            .testTag(SmokeTag.CookScreen)
            .fillMaxSize()
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            RouteHeader(
                title = "Cook",
                subtitle = "Review recipes, pantry ideas, and shopping drafts before anything changes.",
                backLabel = "Back",
                onBack = onBack,
            )
        }
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                CookSection.entries.forEach { choice ->
                    OutlinedButton(
                        onClick = { section = choice },
                        modifier = Modifier.weight(1f),
                    ) {
                        Text(if (section == choice) "${choice.label}*" else choice.label)
                    }
                }
            }
        }
        localError?.let { message ->
            item { ErrorCard(title = "Cook action failed", message = message) }
        }
        if (appState.cookActionInFlight) {
            item { InlineStatusCard(title = "Working", message = "Syncing the latest cook and cart state.") }
        }

        when (section) {
            CookSection.Recipes -> {
                if (recipes.isEmpty()) {
                    item {
                        StatusCard(
                            "No recipes yet",
                            "Use Suggestions to find saved recipes that match your pantry, or import recipes on the web client.",
                        )
                    }
                }
                items(recipes, key = { it.id.toString() }) { recipe ->
                    RecipeRow(
                        recipe = recipe,
                        selected = selectedRecipe?.id == recipe.id,
                        onClick = {
                            launchCook {
                                selectedRecipe = appState.loadCookRecipe(recipe.id.toString())
                                preflight = null
                                execution = null
                            }
                        },
                    )
                }
                selectedRecipe?.let { recipe ->
                    item {
                        RecipeDetailCard(
                            recipe = recipe,
                            allowPartial = allowPartial,
                            onAllowPartialChanged = { allowPartial = it },
                            preflight = preflight,
                            execution = execution,
                            onPreflight = {
                                launchCook {
                                    preflight = appState.preflightCookRecipe(recipe.id.toString(), allowPartial)
                                    execution = null
                                }
                            },
                            onExecute = {
                                launchCook {
                                    execution = appState.executeCookRecipe(recipe.id.toString(), allowPartial)
                                    preflight = execution?.plan
                                }
                            },
                        )
                    }
                }
            }
            CookSection.Suggestions -> {
                item {
                    Button(
                        onClick = {
                            launchCook {
                                val response = appState.createPantrySuggestions(generateRecipeIdeas = true)
                                suggestions = response.suggestions
                                suggestionWarnings = response.warnings
                            }
                        },
                        modifier = Modifier.testTag("pantry.suggestions.generate"),
                    ) {
                        Text("Find ideas from pantry")
                    }
                }
                if (suggestions.isEmpty()) {
                    item {
                        StatusCard(
                            title = "No suggestions yet",
                            message = "Generate suggestions after adding stock and recipes. Cookable saved recipes can be reviewed before inventory changes.",
                        )
                    }
                }
                items(suggestions, key = { it.id.toString() }) { suggestion ->
                    PantrySuggestionCard(
                        suggestion = suggestion,
                        onReviewRecipe = { recipeId ->
                            launchCook {
                                selectedRecipe = appState.loadCookRecipe(recipeId)
                                preflight = null
                                execution = null
                                section = CookSection.Recipes
                            }
                        },
                    )
                }
                suggestionWarnings.forEachIndexed { index, warning ->
                    item {
                        Text(
                            warning,
                            modifier = Modifier.testTag("pantry.suggestion.warning.$index"),
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            }
            CookSection.Shopping -> {
                item {
                    StatusCard(
                        title = "Shopping review",
                        message = "Build a draft cart from replenishment rules and review it before anything is submitted to a supplier.",
                    )
                }
                item {
                    Button(
                        onClick = {
                            launchCook {
                                val generated = appState.generateCookCartDraft()
                                cartDraft = generated.draftId?.let { appState.loadCookCartDraft(it.toString()) }
                                order = null
                            }
                        },
                        modifier = Modifier.testTag(SmokeTag.CartGenerate),
                    ) {
                        Text("Build suggested cart")
                    }
                }
                cartDraft?.let { draft ->
                    item {
                        CartDraftCard(
                            appState = appState,
                            draft = draft,
                            order = order,
                            onSubmit = {
                                launchCook { order = appState.submitCookCartDraft(draft.id.toString()) }
                            },
                            onReceive = { line ->
                                val location = appState.locations.firstOrNull()
                                if (line.productId == null || location == null || line.unit == null) {
                                    localError = "Cart line is missing product, unit, or receiving location."
                                } else {
                                    launchCook {
                                        order = appState.receiveCookSupplierOrder(
                                            orderId = order!!.id.toString(),
                                            productId = line.productId.toString(),
                                            locationId = location.id.toString(),
                                            quantity = line.quantity,
                                            unit = line.unit,
                                        )
                                    }
                                }
                            },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun RecipeRow(
    recipe: RecipeSummaryDto,
    selected: Boolean,
    onClick: () -> Unit,
) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .testTag(SmokeTag.recipeRow(recipe.id.toString()))
            .clickable(onClick = onClick),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(recipe.name, style = MaterialTheme.typography.titleMedium)
            Text("${recipe.servingCount} servings - ${recipe.source.value}")
            if (selected) Text("Selected", color = MaterialTheme.colorScheme.primary)
        }
    }
}

@Composable
private fun RecipeDetailCard(
    recipe: RecipeDto,
    allowPartial: Boolean,
    onAllowPartialChanged: (Boolean) -> Unit,
    preflight: RecipeExecutionPreflightResponse?,
    execution: RecipeExecutionResponse?,
    onPreflight: () -> Unit,
    onExecute: () -> Unit,
) {
    Card {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(recipe.name, style = MaterialTheme.typography.titleLarge)
            recipe.description?.let { Text(it) }
            Text("Confidence ${recipe.version.provenance.firstOrNull()?.parserConfidence ?: "manual"}")
            recipe.version.ingredients.forEach { ingredient ->
                Text("- ${ingredient.displayName} ${ingredient.quantity.amount.orEmpty()} ${ingredient.quantity.unit.orEmpty()}")
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = onPreflight) {
                    Text("Review")
                }
                OutlinedButton(onClick = { onAllowPartialChanged(!allowPartial) }) {
                    Text(if (allowPartial) "Partial on" else "Partial off")
                }
            }
            preflight?.let { plan ->
                plan.ingredients.forEach { ingredient ->
                    Text(
                        "${ingredient.displayName ?: ingredient.product.name}: ${ingredient.inventoryQuantity}/${ingredient.requestedQuantity} ${ingredient.requestedUnit}",
                        modifier = Modifier.testTag(SmokeTag.recipePreflightRow(ingredient.lineId ?: ingredient.product.id.toString())),
                    )
                }
                plan.missingIngredients.forEach { ingredient ->
                    Text(
                        "${ingredient.displayName ?: "Ingredient"} missing ${ingredient.missingQuantity} ${ingredient.requestedUnit}",
                        modifier = Modifier.testTag(SmokeTag.recipeMissingRow(ingredient.lineId ?: ingredient.productId?.toString().orEmpty())),
                        color = MaterialTheme.colorScheme.error,
                    )
                }
                Button(
                    onClick = onExecute,
                    enabled = plan.canExecute || allowPartial,
                    modifier = Modifier.testTag(SmokeTag.RecipePreflightExecute),
                ) {
                    Text(if (allowPartial && !plan.canExecute) "Cook partial" else "Cook")
                }
            }
            execution?.let { Text("Cooked ${it.executionId}") }
        }
    }
}

@Composable
private fun PantrySuggestionCard(
    suggestion: PantrySuggestionDto,
    onReviewRecipe: (String) -> Unit,
) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .testTag("pantry.suggestion.row.${suggestion.id}"),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
            ) {
                Text(suggestion.title, style = MaterialTheme.typography.titleMedium)
                Text("${suggestion.score}", color = MaterialTheme.colorScheme.primary)
            }
            suggestion.summary?.let { Text(it) }
            Text(
                if (suggestion.scoreBreakdown.cookable) "Ready to cook" else "Needs ingredients",
                color = if (suggestion.scoreBreakdown.cookable) {
                    MaterialTheme.colorScheme.primary
                } else {
                    MaterialTheme.colorScheme.error
                },
            )
            suggestion.scoreBreakdown.notes.forEach { note ->
                Text(note, style = MaterialTheme.typography.bodySmall)
            }
            suggestion.missing.forEach { missing ->
                Text(
                    "Missing: ${missing.displayName} ${missing.quantity.orEmpty()} ${missing.unit.orEmpty()}".trim(),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            suggestion.recipeId?.let { recipeId ->
                OutlinedButton(
                    onClick = { onReviewRecipe(recipeId.toString()) },
                    modifier = Modifier.testTag("pantry.suggestion.review.${suggestion.id}"),
                ) {
                    Text("Review recipe")
                }
            }
        }
    }
}

@Composable
private fun CartDraftCard(
    appState: QuartermasterAppState,
    draft: SupplierCartDraftDto,
    order: SupplierOrderDto?,
    onSubmit: () -> Unit,
    onReceive: (dev.quartermaster.android.generated.models.SupplierCartLineDto) -> Unit,
) {
    Card {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text("Supplier cart", style = MaterialTheme.typography.titleLarge)
            Text("Status ${draft.status.value} - intervention ${draft.interventionState.value}")
            draft.reviewNotes?.let { Text(it) }
            draft.lines.forEach { line ->
                Text(
                    "${line.supplierItemId}: ${line.quantity} ${line.unit.orEmpty()}",
                    modifier = Modifier.testTag(SmokeTag.cartRow(line.id.toString())),
                )
            }
            Button(
                onClick = onSubmit,
                enabled = order == null,
                modifier = Modifier.testTag(SmokeTag.CartSubmit),
            ) {
                Text("Submit")
            }
            order?.let { submitted ->
                Text("Order ${submitted.status.value}")
                val receivable = draft.lines.firstOrNull { it.productId != null && it.unit != null }
                Button(
                    onClick = { receivable?.let(onReceive) },
                    enabled = receivable != null && appState.locations.isNotEmpty(),
                    modifier = Modifier.testTag(SmokeTag.CartReceive),
                ) {
                    Text("Receive")
                }
            }
        }
    }
}
