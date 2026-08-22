package VerificationCapture {
    part def ControlUnit;
    part testUnit : ControlUnit;

    requirement def <'REQ-VERIFY-001'> ObservableResponse {
        doc /* The control-unit response shall be observable at its boundary. */
        subject unit : ControlUnit;
    }
    requirement observableResponse : ObservableResponse;

    verification def ResponseVerification {
        subject unit : ControlUnit;
        objective {
            verify requirement : ObservableResponse;
            verify observableResponse;
        }
    }

    verification responseVerification : ResponseVerification {
        subject unit = testUnit;
    }
}
