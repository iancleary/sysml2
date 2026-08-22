package InvalidVerificationCapture {
    requirement def ObservableResponse;

    verification def MissingVerifyTerminator {
        objective {
            verify requirement : ObservableResponse
        }
    }
}
