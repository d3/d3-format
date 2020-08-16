declare namespace d3 {
    function formatDefaultLocale(definition: LocaleDefinition): LocaleFormatter
    function format(specifier: string): (value: number) => string
    function formatPrefix(specifier: string, value: number): string

    function formatLocale(definition: LocaleDefinition): LocaleFormatter
    
    function formatSpecifier(specifier: string): FormatSpecifier

    function precisionFixed(step: number): number
    function precisionPrefix(step: number, value: number): number
    function precisionRound(step: number, max: number): number
    
    export class FormatSpecifier {
        constructor(specifier: FormatSpecifier)

        fill?: string
        align?: string
        sign?: string
        symbol?: string
        zero?: boolean
        width?: number
        comma?: boolean
        precision?: number
        trim?: boolean
        type?: string
    }

    export interface LocaleDefinition {
        decimal?: string
        thousands?: string
        grouping?: number[]
        currency?: string[]
        numerals?: string[]
        percent?: string
        minus?: string
        nan?: string
    }

    export interface LocaleFormatter {
        format(specifier: string): (value: number) => string
        formatPrefix(specifier: string, value: number): string
    }
}

export = d3