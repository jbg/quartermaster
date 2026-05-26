import SwiftUI

struct CookView: View {
  @Environment(AppState.self) private var appState
  @State private var selectedSection = CookSection.recipes
  @State private var recipes: [RecipeSummary] = []
  @State private var selectedRecipe: Recipe?
  @State private var preflight: RecipeExecutionPreflight?
  @State private var execution: RecipeExecutionResult?
  @State private var cartRun: ReplenishmentCartRun?
  @State private var cartDraft: SupplierCartDraft?
  @State private var supplierOrder: SupplierOrder?
  @State private var locations: [Location] = []
  @State private var allowPartial = false
  @State private var isLoading = false
  @State private var errorMessage: String?

  enum CookSection: String, CaseIterable, Identifiable {
    case recipes = "Recipes"
    case suggestions = "Suggestions"
    case carts = "Carts"
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
          suggestionPlaceholder
        case .carts:
          cartReview
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
  }

  private var recipeList: some View {
    List {
      Section("Recipes") {
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

  private var suggestionPlaceholder: some View {
    ContentUnavailableView(
      "Suggestions",
      systemImage: "sparkles",
      description: Text("Pantry suggestions will use the same review surfaces before cooking.")
    )
  }

  private var cartReview: some View {
    List {
      Section("Cart run") {
        Button("Generate mock cart") {
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
      }

      if let cartDraft {
        Section("Draft") {
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

  private func loadInitial() async {
    isLoading = true
    defer { isLoading = false }
    do {
      async let recipeTask = appState.api.recipes()
      async let locationTask = appState.api.locations()
      recipes = try await recipeTask
      locations = try await locationTask
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
