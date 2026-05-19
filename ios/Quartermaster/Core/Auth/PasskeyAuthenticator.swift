import AuthenticationServices
import Foundation
import UIKit

@MainActor
final class PasskeyAuthenticator {
  func register(start: PasskeyRegistrationStart) async throws -> Data {
    let options = try PublicKeyOptions(data: start.publicKey)
    let provider = ASAuthorizationPlatformPublicKeyCredentialProvider(
      relyingPartyIdentifier: options.rpID)
    let request = provider.createCredentialRegistrationRequest(
      challenge: options.challenge,
      name: options.userName,
      userID: options.userID
    )
    let credential = try await perform(request)
    guard
      let registration = credential
        as? ASAuthorizationPlatformPublicKeyCredentialRegistration
    else {
      throw APIError.server(status: 0, body: nil)
    }
    return try JSONSerialization.data(withJSONObject: [
      "id": registration.credentialID.base64URLEncodedString(),
      "rawId": registration.credentialID.base64URLEncodedString(),
      "type": "public-key",
      "response": [
        "attestationObject": registration.rawAttestationObject?.base64URLEncodedString() ?? "",
        "clientDataJSON": registration.rawClientDataJSON.base64URLEncodedString(),
      ],
    ])
  }

  func login(start: PasskeyLoginStart) async throws -> Data {
    let options = try PublicKeyOptions(data: start.publicKey)
    let provider = ASAuthorizationPlatformPublicKeyCredentialProvider(
      relyingPartyIdentifier: options.rpID)
    let request = provider.createCredentialAssertionRequest(challenge: options.challenge)
    let credential = try await perform(request)
    guard
      let assertion = credential
        as? ASAuthorizationPlatformPublicKeyCredentialAssertion
    else {
      throw APIError.server(status: 0, body: nil)
    }
    return try JSONSerialization.data(withJSONObject: [
      "id": assertion.credentialID.base64URLEncodedString(),
      "rawId": assertion.credentialID.base64URLEncodedString(),
      "type": "public-key",
      "response": [
        "authenticatorData": assertion.rawAuthenticatorData.base64URLEncodedString(),
        "clientDataJSON": assertion.rawClientDataJSON.base64URLEncodedString(),
        "signature": assertion.signature.base64URLEncodedString(),
        "userHandle": assertion.userID.base64URLEncodedString(),
      ],
    ])
  }

  private func perform(_ request: ASAuthorizationRequest) async throws -> ASAuthorizationCredential
  {
    try await withCheckedThrowingContinuation { continuation in
      let delegate = AuthorizationDelegate(continuation: continuation)
      let controller = ASAuthorizationController(authorizationRequests: [request])
      controller.delegate = delegate
      controller.presentationContextProvider = delegate
      AuthorizationDelegate.active = delegate
      controller.performRequests()
    }
  }
}

private final class AuthorizationDelegate: NSObject, ASAuthorizationControllerDelegate,
  ASAuthorizationControllerPresentationContextProviding
{
  static var active: AuthorizationDelegate?
  let continuation: CheckedContinuation<ASAuthorizationCredential, Error>

  init(continuation: CheckedContinuation<ASAuthorizationCredential, Error>) {
    self.continuation = continuation
  }

  func authorizationController(
    controller: ASAuthorizationController,
    didCompleteWithAuthorization authorization: ASAuthorization
  ) {
    Self.active = nil
    continuation.resume(returning: authorization.credential)
  }

  func authorizationController(
    controller: ASAuthorizationController,
    didCompleteWithError error: Error
  ) {
    Self.active = nil
    continuation.resume(throwing: error)
  }

  func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
    UIApplication.shared.connectedScenes
      .compactMap { $0 as? UIWindowScene }
      .flatMap(\.windows)
      .first { $0.isKeyWindow } ?? ASPresentationAnchor()
  }
}

private struct PublicKeyOptions {
  let rpID: String
  let challenge: Data
  let userName: String
  let userID: Data

  init(data: Data) throws {
    guard
      let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
      let rp = root["rp"] as? [String: Any],
      let rpID = rp["id"] as? String,
      let challengeString = root["challenge"] as? String,
      let challenge = Data(base64URLEncoded: challengeString),
      let user = root["user"] as? [String: Any],
      let userName = user["name"] as? String,
      let userIDString = user["id"] as? String,
      let userID = Data(base64URLEncoded: userIDString)
    else {
      throw APIError.server(status: 0, body: nil)
    }
    self.rpID = rpID
    self.challenge = challenge
    self.userName = user["displayName"] as? String ?? userName
    self.userID = userID
  }
}

extension Data {
  init?(base64URLEncoded value: String) {
    var base64 = value.replacingOccurrences(of: "-", with: "+")
      .replacingOccurrences(of: "_", with: "/")
    while base64.count % 4 != 0 {
      base64.append("=")
    }
    self.init(base64Encoded: base64)
  }

  func base64URLEncodedString() -> String {
    base64EncodedString()
      .replacingOccurrences(of: "+", with: "-")
      .replacingOccurrences(of: "/", with: "_")
      .replacingOccurrences(of: "=", with: "")
  }
}
