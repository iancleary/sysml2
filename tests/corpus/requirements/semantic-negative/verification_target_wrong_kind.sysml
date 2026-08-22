package VerificationTargetWrongKind {
    part def Computer;
    part computer : Computer;
    verification def ComputerVerification {
        subject tested : Computer;
        objective {
            verify computer;
        }
    }
}
