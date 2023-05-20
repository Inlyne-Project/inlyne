
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'inlyne' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'inlyne'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'inlyne' {
            [CompletionResult]::new('-t', 't', [CompletionResultType]::ParameterName, 'Theme to use when rendering')
            [CompletionResult]::new('--theme', 'theme', [CompletionResultType]::ParameterName, 'Theme to use when rendering')
            [CompletionResult]::new('-s', 's', [CompletionResultType]::ParameterName, 'Factor to scale rendered file by [default: OS defined window scale factor]')
            [CompletionResult]::new('--scale', 'scale', [CompletionResultType]::ParameterName, 'Factor to scale rendered file by [default: OS defined window scale factor]')
            [CompletionResult]::new('-c', 'c', [CompletionResultType]::ParameterName, 'Configuration file to use')
            [CompletionResult]::new('--config', 'config', [CompletionResultType]::ParameterName, 'Configuration file to use')
            [CompletionResult]::new('-w', 'w', [CompletionResultType]::ParameterName, 'Maximum width of page in pixels')
            [CompletionResult]::new('--page-width', 'page-width', [CompletionResultType]::ParameterName, 'Maximum width of page in pixels')
            [CompletionResult]::new('-h', 'h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', 'help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', 'V', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', 'version', [CompletionResultType]::ParameterName, 'Print version')
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
