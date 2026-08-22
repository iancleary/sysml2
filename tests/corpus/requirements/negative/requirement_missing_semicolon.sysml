package InvalidRequirementCapture {
    part def ControlledUnit;

    requirement def <'REQ-CAPTURE-NEG-001'> MissingSubjectTerminator {
        doc /* This fixture contains exactly one intentional syntax fault. */
        subject unit : ControlledUnit
        require constraint responsive;
    }
}
