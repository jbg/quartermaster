import SwiftUI

struct CookView: View {
  @Environment(AppState.self) private var appState
  @State private var selectedSection = CookSection.suggestions
  @State private var recipes: [RecipeSummary] = []
  @State private var selectedRecipe: Recipe?
  @State private var preflight: RecipeExecutionPreflight?
  @State private var execution: RecipeExecutionResult?
  @State private var suggestions: [PantrySuggestion] = []
  @State private var selectedSuggestion: PantrySuggestion?
  @State private var suggestionWarnings: [String] = []
  @State private var mealPlans: [MealPlanSummary] = []
  @State private var selectedMealPlan: MealPlan?
  @State private var mealPlanDates: [String] = []
  @State private var mealPlanRangeStart = Date()
  @State private var mealPlanRangeEnd = Date()
  @State private var mealPlanTitle = ""
  @State private var canGenerateRecipeIdeas = false
  @State private var cartRun: ReplenishmentCartRun?
  @State private var cartDraft: SupplierCartDraft?
  @State private var supplierOrder: SupplierOrder?
  @State private var locations: [Location] = []
  @State private var allowPartial = false
  @State private var isLoading = false
  @State private var isGeneratingSuggestions = false
  @State private var isGeneratingMealPlan = false
  @State private var errorMessage: String?

  enum CookSection: String, CaseIterable, Identifiable {
    case suggestions = "Suggestions"
    case recipes = "Recipes"
    case mealPlans = "Meal Plans"
    case shopping = "Shopping"
    var id: String { rawValue }
  }

  var body: some View {
    VStack(spacing: 0) {
      Picker("Cook section", selection: $selectedSection) {
        ForEach(CookSection.allCases) { section in
          Text(section.rawValue).tag(section)
        }
      }
      .pickerStyle(.segmented)
      .padding()
      .accessibilityIdentifier("cook.segmented")

      Group {
        switch selectedSection {
        case .recipes:
          recipeList
        case .suggestions:
          suggestionList
        case .mealPlans:
          mealPlanList
        case .shopping:
          shoppingReview
        }
      }
    }
    .navigationTitle("Cook")
    .task { await loadInitial() }
    .refreshable { await loadInitial() }
    .alert(
      "Cook flow error",
      isPresented: Binding(get: { errorMessage != nil }, set: { _ in errorMessage = nil })
    ) {
      Button("OK", role: .cancel) { errorMessage = nil }
    } message: {
      Text(errorMessage ?? "")
    }
    .sheet(item: $selectedSuggestion) { suggestion in
      PantrySuggestionDetailView(suggestion: suggestion)
    }
  }

  private var recipeList: some View {
    List {
      Section("Recipes") {
        if recipes.isEmpty {
          VStack(alignment: .leading, spacing: 8) {
            Text("No recipes yet")
              .font(.headline)
            Text(
              "Saved recipes you create or import show up here for review before cooking."
            )
            .font(.subheadline)
            .foregroundStyle(.secondary)
          }
        } else {
          ForEach(recipes) { recipe in
            Button {
              Task { await openRecipe(recipe.id) }
            } label: {
              VStack(alignment: .leading) {
                Text(recipe.name)
                Text("\(recipe.servingCount) servings")
                  .font(.caption)
                  .foregroundStyle(.secondary)
              }
            }
            .accessibilityIdentifier("recipe.row.\(recipe.id)")
          }
        }
      }

      if let selectedRecipe {
        Section("Review plan") {
          ForEach(selectedRecipe.version.ingredients) { ingredient in
            HStack {
              Text(ingredient.displayName)
              Spacer()
              Text(ingredient.displayQuantity)
                .foregroundStyle(.secondary)
            }
          }
          Button("Run preflight") {
            Task { await runPreflight() }
          }
          .accessibilityIdentifier("recipe.preflight.run")
        }
      }

      if let preflight {
        Section(preflight.canExecute ? "Ready to cook" : "Needs review") {
          ForEach(Array(preflight.ingredients.enumerated()), id: \.offset) { index, ingredient in
            VStack(alignment: .leading) {
              Text(ingredient.displayName ?? ingredient.product.name)
              Text("\(ingredient.inventoryQuantity) \(ingredient.inventoryUnit)")
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .accessibilityIdentifier("recipe.preflight.row.\(ingredient.lineId ?? String(index))")
          }
          ForEach(Array(preflight.missingIngredients.enumerated()), id: \.offset) {
            index,
            missing in
            VStack(alignment: .leading) {
              Text(missing.displayName ?? "Missing ingredient")
              Text("\(missing.missingQuantity) \(missing.requestedUnit) missing")
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .accessibilityIdentifier("recipe.missing.row.\(missing.lineId ?? String(index))")
          }
          if !preflight.canExecute {
            Toggle("Confirm partial execution", isOn: $allowPartial)
              .accessibilityIdentifier("recipe.partial.confirm")
          }
          Button("Cook recipe") {
            Task { await executeRecipe() }
          }
          .disabled(isLoading || (!preflight.canExecute && !allowPartial))
          .accessibilityIdentifier("recipe.preflight.execute")
        }
      }

      if let execution {
        Section("Executed") {
          Text(execution.executionId)
            .accessibilityIdentifier("recipe.execution.result")
        }
      }
    }
    .accessibilityIdentifier("recipe.list")
  }

  private var suggestionList: some View {
    List {
      Section {
        Button {
          Task { await generateSuggestions() }
        } label: {
          if isGeneratingSuggestions {
            HStack {
              ProgressView()
              Text("Finding ideas from pantry...")
            }
          } else {
            Label("Find ideas from pantry", systemImage: "sparkles")
          }
        }
        .disabled(isLoading || isGeneratingSuggestions)
        .accessibilityIdentifier("pantry.suggestions.generate")
      } footer: {
        Text(suggestionFooter)
      }

      Section("Suggested to cook") {
        if isGeneratingSuggestions {
          HStack {
            ProgressView()
            Text("Generating pantry ideas...")
              .foregroundStyle(.secondary)
          }
          .accessibilityIdentifier("pantry.suggestions.generating")
        } else if suggestions.isEmpty {
          VStack(alignment: .leading, spacing: 8) {
            Text("No suggestions yet")
              .font(.headline)
            Text(
              "Generate suggestions after adding stock and recipes. Cookable saved recipes can be reviewed here before inventory is changed."
            )
            .font(.subheadline)
            .foregroundStyle(.secondary)
          }
        } else {
          ForEach(suggestions) { suggestion in
            Button {
              openSuggestion(suggestion)
            } label: {
              PantrySuggestionRow(suggestion: suggestion)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("pantry.suggestion.row.\(suggestion.id)")
          }
        }
      }

      if !suggestionWarnings.isEmpty {
        Section("Warnings") {
          ForEach(suggestionWarnings, id: \.self) { warning in
            Text(warning)
          }
        }
      }
    }
    .accessibilityIdentifier("pantry.suggestion.list")
  }

  private var suggestionFooter: String {
    if canGenerateRecipeIdeas {
      return
        "Suggestions rank saved recipes against your current stock and can include new recipe ideas."
    }
    return "Suggestions rank saved recipes against your current stock."
  }

  private var shoppingReview: some View {
    List {
      Section {
        Button("Build suggested cart") {
          Task { await generateCart() }
        }
        .accessibilityIdentifier("cart.generate")
        if let cartRun {
          HStack {
            Text(cartRun.guardrailDecision.rawValue.replacingOccurrences(of: "_", with: " "))
            Spacer()
            Text(cartRun.status.rawValue.replacingOccurrences(of: "_", with: " "))
              .foregroundStyle(.secondary)
          }
          .accessibilityIdentifier("cart.guardrail.banner")
        }
      } header: {
        Text("Shopping review")
      } footer: {
        Text(
          "Build a draft cart from replenishment rules and review it before anything is submitted to a supplier."
        )
      }

      if let cartDraft {
        Section("Cart to approve") {
          ForEach(cartDraft.lines) { line in
            VStack(alignment: .leading) {
              Text(line.supplierItemId)
              Text("\(line.quantity) \(line.unit ?? "")")
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .accessibilityIdentifier("cart.draft.line.\(line.id)")
          }
          Button("Submit cart") {
            Task { await submitCart() }
          }
          .disabled(isLoading || cartDraft.status == .submitted)
          .accessibilityIdentifier("cart.submit")
        }
      }

      if let supplierOrder {
        Section("Order") {
          Text(supplierOrder.status.rawValue.replacingOccurrences(of: "_", with: " "))
            .accessibilityIdentifier("cart.order.result")
          Button("Receive first line") {
            Task { await receiveOrder() }
          }
          .disabled(isLoading || locations.isEmpty)
          .accessibilityIdentifier("cart.receive.submit")
        }
      }
    }
    .accessibilityIdentifier("cart.review")
  }

  private var mealPlanList: some View {
    List {
      Section("Generate") {
        TextField("Title", text: $mealPlanTitle)
        DatePicker("Start", selection: $mealPlanRangeStart, displayedComponents: .date)
        DatePicker("End", selection: $mealPlanRangeEnd, displayedComponents: .date)
        Button("Add date range") { addMealPlanDateRange() }
          .disabled(isGeneratingMealPlan)
        if mealPlanDates.isEmpty {
          Text("Add a date range, then remove any dates you do not need.")
            .font(.caption)
            .foregroundStyle(.secondary)
        } else {
          ForEach(mealPlanDates, id: \.self) { date in
            HStack {
              Text(date)
              Spacer()
              Button("Remove") { mealPlanDates.removeAll { $0 == date } }
                .buttonStyle(.borderless)
                .disabled(isGeneratingMealPlan)
            }
          }
        }
        Button {
          Task { await generateMealPlan() }
        } label: {
          if isGeneratingMealPlan {
            HStack {
              ProgressView()
              Text("Generating meal plan...")
            }
          } else {
            Label("Generate meal plan", systemImage: "sparkles")
          }
        }
        .disabled(isLoading || isGeneratingMealPlan || mealPlanDates.isEmpty)
        .accessibilityIdentifier("meal.plan.generate")
        if isGeneratingMealPlan {
          HStack {
            ProgressView()
            Text("Checking recipes and stock. This can take a little while.")
              .foregroundStyle(.secondary)
          }
          .accessibilityIdentifier("meal.plan.generating")
        }
      }

      Section("Plans") {
        if mealPlans.isEmpty {
          VStack(alignment: .leading, spacing: 8) {
            Text("No meal plans yet")
              .font(.headline)
            Text("Choose non-contiguous dates and reserve stock for planned meals.")
              .font(.subheadline)
              .foregroundStyle(.secondary)
          }
        } else {
          ForEach(mealPlans) { plan in
            Button {
              Task { await openMealPlan(plan.id) }
            } label: {
              VStack(alignment: .leading) {
                Text(plan.title)
                Text("\(plan.dates.joined(separator: ", ")) - \(plan.mealCount) meals")
                  .font(.caption)
                  .foregroundStyle(.secondary)
              }
            }
            .accessibilityIdentifier("meal.plan.row.\(plan.id)")
          }
        }
      }

      if let selectedMealPlan {
        MealPlanDetailSection(
          plan: selectedMealPlan,
          isLoading: isLoading,
          onRefresh: { Task { await refreshMealPlan(selectedMealPlan.id) } },
          onCook: { mealID in
            Task { await cookMealPlanMeal(planID: selectedMealPlan.id, mealID: mealID) }
          },
          onSkip: { mealID in
            Task { await skipMealPlanMeal(planID: selectedMealPlan.id, mealID: mealID) }
          })
      }
    }
    .accessibilityIdentifier("meal.plan.list")
  }

  private func loadInitial() async {
    isLoading = true
    defer { isLoading = false }
    do {
      async let recipeTask = appState.api.recipes()
      async let locationTask = appState.api.locations()
      async let suggestionTask = appState.api.pantrySuggestions()
      async let mealPlanTask = appState.api.mealPlans()
      async let aiStatusTask = appState.api.aiStatus()
      recipes = try await recipeTask
      locations = try await locationTask
      suggestions = try await suggestionTask
      mealPlans = try await mealPlanTask
      if let aiStatus = try? await aiStatusTask {
        canGenerateRecipeIdeas = aiStatus.enabled && aiStatus.configured
      } else {
        canGenerateRecipeIdeas = false
      }
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func openRecipe(_ id: String) async {
    do {
      selectedRecipe = try await appState.api.getRecipe(id: id)
      preflight = nil
      execution = nil
      allowPartial = false
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func openSuggestion(_ suggestion: PantrySuggestion) {
    if let recipeID = suggestion.recipeId {
      selectedSection = .recipes
      Task { await openRecipe(recipeID) }
    } else {
      selectedSuggestion = suggestion
    }
  }

  private func runPreflight() async {
    guard let selectedRecipe else { return }
    do {
      preflight = try await appState.api.preflightRecipe(selectedRecipe, allowPartial: false)
      allowPartial = false
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func executeRecipe() async {
    guard let selectedRecipe else { return }
    do {
      execution = try await appState.api.executeRecipe(selectedRecipe, allowPartial: allowPartial)
      preflight = execution?.plan
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func generateSuggestions() async {
    isLoading = true
    isGeneratingSuggestions = true
    defer {
      isGeneratingSuggestions = false
      isLoading = false
    }
    do {
      let response = try await appState.api.createPantrySuggestions(
        generateRecipeIdeas: canGenerateRecipeIdeas)
      suggestions = response.suggestions
      suggestionWarnings = response.warnings
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func generateMealPlan() async {
    isLoading = true
    isGeneratingMealPlan = true
    defer {
      isGeneratingMealPlan = false
      isLoading = false
    }
    do {
      let plan = try await appState.api.generateMealPlan(
        title: mealPlanTitle.isEmpty ? nil : mealPlanTitle,
        dates: mealPlanDates)
      selectedMealPlan = plan
      mealPlans = try await appState.api.mealPlans()
      mealPlanTitle = ""
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func addMealPlanDateRange() {
    let calendar = Calendar(identifier: .gregorian)
    let start = calendar.startOfDay(for: mealPlanRangeStart)
    let end = calendar.startOfDay(for: mealPlanRangeEnd)
    guard end >= start else {
      errorMessage = "End date must be on or after start date."
      return
    }
    let dayCount = (calendar.dateComponents([.day], from: start, to: end).day ?? 0) + 1
    guard dayCount <= 90 else {
      errorMessage = "Choose a range of 90 days or fewer."
      return
    }

    var selected = Set(mealPlanDates)
    for offset in 0..<dayCount {
      if let date = calendar.date(byAdding: .day, value: offset, to: start) {
        selected.insert(Self.dateFormatter.string(from: date))
      }
    }
    mealPlanDates = selected.sorted()
  }

  private func openMealPlan(_ id: String) async {
    do {
      selectedMealPlan = try await appState.api.getMealPlan(id: id)
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func refreshMealPlan(_ id: String) async {
    do {
      selectedMealPlan = try await appState.api.refreshMealPlan(id: id)
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func cookMealPlanMeal(planID: String, mealID: String) async {
    do {
      _ = try await appState.api.executeMealPlanMeal(planID: planID, mealID: mealID)
      selectedMealPlan = try await appState.api.getMealPlan(id: planID)
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func skipMealPlanMeal(planID: String, mealID: String) async {
    do {
      selectedMealPlan = try await appState.api.skipMealPlanMeal(planID: planID, mealID: mealID)
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private static let dateFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.calendar = Calendar(identifier: .gregorian)
    formatter.locale = Locale(identifier: "en_US_POSIX")
    formatter.dateFormat = "yyyy-MM-dd"
    return formatter
  }()

  private func generateCart() async {
    do {
      let response = try await appState.api.generateCartDraft()
      cartRun = response.run
      if let draftID = response.draftId {
        cartDraft = try await appState.api.getSupplierCartDraft(id: draftID)
      } else {
        cartDraft = nil
      }
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func submitCart() async {
    guard let cartDraft else { return }
    do {
      supplierOrder = try await appState.api.submitSupplierCartDraft(id: cartDraft.id)
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func receiveOrder() async {
    guard
      let supplierOrder,
      let productID = cartDraft?.lines.first(where: { $0.productId != nil })?.productId,
      let locationID = locations.first?.id
    else { return }
    do {
      self.supplierOrder = try await appState.api.receiveSupplierOrder(
        id: supplierOrder.id,
        productID: productID,
        locationID: locationID)
    } catch {
      errorMessage = userMessage(error)
    }
  }

  private func userMessage(_ error: Error) -> String {
    (error as? APIError)?.userFacingMessage ?? error.localizedDescription
  }
}

private struct MealPlanDetailSection: View {
  let plan: MealPlan
  let isLoading: Bool
  let onRefresh: () -> Void
  let onCook: (String) -> Void
  let onSkip: (String) -> Void

  var body: some View {
    Section {
      Button("Refresh reservations", action: onRefresh)
        .disabled(isLoading)
      ForEach(plan.days) { day in
        VStack(alignment: .leading, spacing: 10) {
          Text(day.date)
            .font(.headline)
          ForEach(day.meals) { meal in
            VStack(alignment: .leading, spacing: 6) {
              HStack {
                Text(meal.slotLabel)
                  .font(.subheadline.weight(.semibold))
                Spacer()
                Text(meal.status)
                  .font(.caption)
                  .foregroundStyle(meal.status == "conflicted" ? .orange : .secondary)
              }
              Text(meal.recipeName ?? "Unassigned meal")
              if !meal.reservations.isEmpty {
                Text("\(meal.reservations.count) stock reservations")
                  .font(.caption)
                  .foregroundStyle(.secondary)
              }
              HStack {
                Button("Cook") { onCook(meal.id) }
                  .disabled(isLoading || meal.status != "planned")
                Button("Skip") { onSkip(meal.id) }
                  .disabled(isLoading || meal.status == "skipped")
              }
              .buttonStyle(.borderless)
            }
            .padding(.vertical, 4)
          }
        }
      }
    } header: {
      Text(plan.title)
    }
  }
}

private struct PantrySuggestionRow: View {
  let suggestion: PantrySuggestion

  var body: some View {
    HStack(alignment: .center, spacing: 12) {
      VStack(alignment: .leading, spacing: 8) {
        Text(suggestion.title)
          .font(.headline)
        if let summary = suggestion.summary {
          Text(summary)
            .font(.subheadline)
            .foregroundStyle(.secondary)
        }
        Label(statusText, systemImage: statusImage)
          .font(.caption)
          .foregroundStyle(statusColor)
        ForEach(Array(suggestion.missing.enumerated()), id: \.offset) { _, missing in
          Text("Missing: \(missing.displayText)")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }
      Spacer(minLength: 8)
      Image(systemName: "chevron.right")
        .font(.caption.weight(.semibold))
        .foregroundStyle(.tertiary)
        .accessibilityHidden(true)
    }
    .contentShape(Rectangle())
  }

  private var statusText: String {
    suggestion.scoreBreakdown.cookable ? "Ready to cook" : "Needs ingredients"
  }

  private var statusImage: String {
    suggestion.scoreBreakdown.cookable ? "checkmark.circle.fill" : "exclamationmark.circle.fill"
  }

  private var statusColor: Color {
    suggestion.scoreBreakdown.cookable ? .green : .orange
  }
}

private struct PantrySuggestionDetailView: View {
  @Environment(\.dismiss) private var dismiss
  let suggestion: PantrySuggestion

  var body: some View {
    NavigationStack {
      List {
        Section {
          if let summary = suggestion.summary {
            Text(summary)
          }
          Label(statusText, systemImage: statusImage)
            .foregroundStyle(statusColor)
        }

        if let idea = suggestion.generatedRecipe {
          generatedRecipeSections(idea.value1)
        } else {
          Section {
            Text("Open this saved recipe from the Recipes section to review the cooking plan.")
              .foregroundStyle(.secondary)
          }
        }

        if !suggestion.missing.isEmpty {
          Section("Missing ingredients") {
            ForEach(Array(suggestion.missing.enumerated()), id: \.offset) { _, missing in
              Text(missing.displayText)
            }
          }
        }

        if !suggestion.scoreBreakdown.notes.isEmpty {
          Section("Why suggested") {
            ForEach(suggestion.scoreBreakdown.notes, id: \.self) { note in
              Text(note)
            }
          }
        }
      }
      .navigationTitle(suggestion.title)
      .navigationBarTitleDisplayMode(.inline)
      .toolbar {
        ToolbarItem(placement: .cancellationAction) {
          Button("Done") { dismiss() }
        }
      }
    }
  }

  @ViewBuilder
  private func generatedRecipeSections(_ idea: GeneratedRecipeIdea) -> some View {
    if let explanation = idea.explanation {
      Section("Idea") {
        Text(explanation)
      }
    }

    Section("Servings") {
      Text(idea.servingCount)
    }

    let ingredients = idea.ingredients ?? []
    if !ingredients.isEmpty {
      Section("Ingredients") {
        ForEach(Array(ingredients.enumerated()), id: \.offset) { _, ingredient in
          VStack(alignment: .leading, spacing: 4) {
            Text(ingredient.displayName)
            Text(ingredient.displayQuantity)
              .font(.caption)
              .foregroundStyle(.secondary)
          }
        }
      }
    }

    let steps = idea.steps ?? []
    if !steps.isEmpty {
      Section("Steps") {
        ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
          VStack(alignment: .leading, spacing: 4) {
            Text("Step \(index + 1)")
              .font(.caption)
              .foregroundStyle(.secondary)
            Text(step.instruction)
          }
        }
      }
    }

    let substitutions = idea.substitutions ?? []
    if !substitutions.isEmpty {
      Section("Substitutions") {
        ForEach(substitutions, id: \.self) { substitution in
          Text(substitution)
        }
      }
    }

    let unresolvedConversions = idea.unresolvedConversions ?? []
    if !unresolvedConversions.isEmpty {
      Section("Conversion notes") {
        ForEach(unresolvedConversions, id: \.self) { note in
          Text(note)
        }
      }
    }
  }

  private var statusText: String {
    suggestion.scoreBreakdown.cookable ? "Ready to cook" : "Needs ingredients"
  }

  private var statusImage: String {
    suggestion.scoreBreakdown.cookable ? "checkmark.circle.fill" : "exclamationmark.circle.fill"
  }

  private var statusColor: Color {
    suggestion.scoreBreakdown.cookable ? .green : .orange
  }
}

extension PantrySuggestionMissing {
  fileprivate var displayText: String {
    "\(displayName)\(quantity.map { " \($0)" } ?? "")\(unit.map { " \($0)" } ?? "")"
  }
}
