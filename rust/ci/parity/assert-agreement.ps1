#Requires -Version 5.1

<#
.SYNOPSIS
    Throws unless a differential summary reports exact agreement.

.DESCRIPTION
    rust/tools/differential.mjs writes a machine-readable summary of a run. This is
    the precondition ci/parity/collect-differential.ps1 applies before archiving
    one: an artifact that records a disagreement, or a run that never finished, is
    not evidence of parity and must not be committed as if it were.

    It is a separate script rather than a function inside the collector so that it
    can be exercised on its own, against a summary that fails. An assertion nobody
    has seen fail is not known to work, which is the same standard section 6.3
    applies to the assertion step in .github/workflows/prerelease-differential.yml.

    The scope is deliberately narrower than that workflow step. This says "the run
    agreed", which is true of every corpus. The workflow additionally enforces
    section 6.3's ten-million output-comparison floor, the drawn-case equality and
    the per-locale floor, which are properties of the pre-release corpus in
    particular. The counts this script reads are recorded verbatim in the archive,
    so nothing it declines to assert is lost.

.PARAMETER SummaryPath
    The summary written by `differential.mjs --summary`.

.EXAMPLE
    ./ci/parity/assert-agreement.ps1 -SummaryPath target/differential/ci-summary.json
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $SummaryPath
)

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $SummaryPath -PathType Leaf)) {
    # ${...} is required: PowerShell reads `$SummaryPath:` as a drive-qualified
    # variable and refuses to parse the file at all.
    throw "no differential summary at ${SummaryPath}: the run did not complete"
}

try {
    $summary = Get-Content -LiteralPath $SummaryPath -Raw | ConvertFrom-Json
}
catch {
    throw "$SummaryPath is not valid JSON: $($_.Exception.Message)"
}

$problems = @()

# Every field is addressed defensively. A summary truncated by a killed run parses
# as JSON but has no `cases` object, and Set-StrictMode turns reading an absent
# property into a terminating error rather than into $null -- which would otherwise
# compare equal to 0 and pass.
foreach ($property in @('schema', 'corpus', 'seed', 'status', 'cases', 'by_section')) {
    if (-not $summary.PSObject.Properties.Name.Contains($property)) {
        $problems += "the summary has no '$property' field, so it is not a complete differential summary"
    }
}

if ($problems.Count -eq 0) {
    if ($summary.status -ne 'agreement') {
        $problems += "status is '$($summary.status)', not 'agreement'"
    }
    if ([long] $summary.cases.findings -ne 0) {
        $problems += "$($summary.cases.findings) disagreement(s)"
    }
    if ([long] $summary.cases.unimplemented -ne 0) {
        $problems += "$($summary.cases.unimplemented) case(s) reported unimplemented"
    }
    if ([long] $summary.cases.asked -le 0) {
        $problems += 'the run compared no cases'
    }
    if ($summary.PSObject.Properties.Name.Contains('census_problems') -and
        @($summary.census_problems).Count -ne 0) {
        $problems += @($summary.census_problems)
    }
}

if ($problems.Count -ne 0) {
    foreach ($problem in $problems) { Write-Host "FAIL: $problem" }
    throw "$SummaryPath does not report exact agreement"
}

Write-Host ("    agreement: corpus $($summary.corpus), seed $($summary.seed)," +
    " $($summary.cases.asked) case(s), 0 disagreement(s)")
