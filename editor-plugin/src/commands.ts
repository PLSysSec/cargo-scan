import * as vscode from 'vscode';
import { EffectResponseData, EffectsResponse } from './file_tree_view';
import { annotations, client, locationsProvider } from './extension';
import { AuditResponse } from './audit_annotations';
import { convertLocation } from './util';

export function registerCommands(context: vscode.ExtensionContext) {
    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.scan', async () => {
            const response = await client.sendRequest<EffectsResponse>('cargo-scan.scan');
            context.globalState.update('annotateEffects', false);
            context.globalState.update('chainAudit', false);
            
            const effects = response.effects.map(effect => ({
                ...effect,
                location: convertLocation(effect.location)
            }));

            locationsProvider.clear();
            locationsProvider.setLocations(effects);
            annotations.clear();
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.audit', async () => {
            const response = await client.sendRequest<AuditResponse>('cargo-scan.audit');
            context.globalState.update('annotateEffects', true);
            context.globalState.update('chainAudit', false);

            let effectsMap = new Map<EffectResponseData, string>();    
            
            response.effects.forEach(x => {
                const location = convertLocation(x[0].location);
                effectsMap.set({ ...x[0], location }, x[1]);
            });
            
            locationsProvider.clear();
            locationsProvider.setLocations([...effectsMap.keys()]);                       
            annotations.setPreviousAnnotations(locationsProvider.getGroupedEffects(), effectsMap);
        })
    );  
    
    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.safeAnnotation', async (effect: EffectResponseData) => {
            const chain_audit_mode = context.globalState.get('chainAudit');
            annotations.trackUserAnnotations(effect, 'Safe');
            const eff = { ...effect, location: { uri: effect.location.uri.toString(), range: effect.location.range }};

            // Notify server about the received safety annotation from the user
            client.sendNotification('cargo-scan.set_annotation', { safety_annotation: 'Safe', effect: eff, chain_audit_mode });
            
            // If we're annotating effects in a chain audit,
            // reload chain to update the previewed effects
            if (chain_audit_mode) {
                vscode.commands.executeCommand('cargo-scan.audit_chain');
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.unsafeAnnotation', async (effect: EffectResponseData) => {
            const chain_audit_mode = context.globalState.get('chainAudit');
            annotations.trackUserAnnotations(effect, 'Unsafe');  
            const eff = { ...effect, location: { uri: effect.location.uri.toString(), range: effect.location.range }};

            // Notify server about the received safety annotation from the user
            client.sendNotification('cargo-scan.set_annotation', { safety_annotation: 'Unsafe', effect: eff, chain_audit_mode }); 
            
            // If we're annotating effects in a chain audit,
            // reload chain to update the previewed effects
            if (chain_audit_mode) {
                vscode.commands.executeCommand('cargo-scan.audit_chain');
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.get_callers', async (effect: EffectResponseData) => {
            const response = await client.sendRequest<EffectsResponse>('cargo-scan.get_callers', 
                { ...effect, location: { uri: effect.location.uri.toString(), range: effect.location.range }});
            
            const callers = response.effects.map(effect => ({
                ...effect,
                location: convertLocation(effect.location)
            }));

            locationsProvider.setLocations(callers);
            annotations.trackUserAnnotations(effect, 'Caller-checked');
            annotations.showCallers(effect, callers);
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.create_chain', async () => {
            client.sendRequest('cargo-scan.create_chain');
            context.globalState.update('annotateEffects', false);
            context.globalState.update('chainAudit', false);
            locationsProvider.clear();
            annotations.clear();         
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('cargo-scan.audit_chain', async () => {
            const response = await client.sendRequest<AuditResponse>('cargo-scan.audit_chain');
            context.globalState.update('annotateEffects', true);
            context.globalState.update('chainAudit', true);
            let effectsMap = new Map<EffectResponseData, string>();    
            
            response.effects.forEach(x => {
                const location = convertLocation(x[0].location);
                effectsMap.set({ ...x[0], location }, x[1]);
            });
            
            locationsProvider.clear();
            locationsProvider.setLocations([...effectsMap.keys()]);                       
            annotations.setPreviousAnnotations(locationsProvider.getGroupedEffects(), effectsMap);
        })
    );
}