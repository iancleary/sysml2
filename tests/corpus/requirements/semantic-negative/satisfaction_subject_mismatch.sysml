package SatisfactionSubjectMismatch {
    part def Computer;
    part def Valve;
    part valve : Valve;
    requirement def ComputerRequirement {
        subject computer : Computer;
    }
    requirement selected : ComputerRequirement;
    satisfy selected by valve;
}
