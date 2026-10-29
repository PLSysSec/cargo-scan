import * as vscode from 'vscode';
import * as path from 'path';
import { DecorationProvider } from './decorations';

export interface EffectResponseData {
    caller: string;
    callee: string;
    effect_type: string;
    location: vscode.Location;
    crate_name: string;
}

export interface EffectsResponse {
    effects: EffectResponseData[];
}

export interface EffectTreeResponse {
    info: EffectResponseData;
    annotation: string;
    children: EffectTreeResponse[];
}

export interface DepRankInfo {
    crate_name:   string;
    topo_index:   number;
    base_effects: number;
    propagated_effects: number;
}

// { funcName -> EffectTreeResponse[] } — multiple trees can share the same root function
type AuditFuncMap = { [funcName: string]: EffectTreeResponse[] };
type AuditFileMap = { [file: string]: AuditFuncMap };

// Collect every root-to-leaf path in the tree (preserves node references).
function collectPaths(tree: EffectTreeResponse): EffectTreeResponse[][] {
    if (tree.children.length === 0) return [[tree]];
    return tree.children.flatMap(child =>
        collectPaths(child).map(path => [tree, ...path])
    );
}

// Build a EffectTreeResponse chain from a path where path[0] is the new root
function pathToInvertedTree(path: EffectTreeResponse[]): EffectTreeResponse {
    return path.reduceRight<EffectTreeResponse | null>((child, node) => ({
        info: node.info,
        annotation: node.annotation,
        children: child ? [child] : []
    }), null)!;
}

// Invert an EffectTree so the outermost CallerChecked caller becomes the root.
// A non-propagated Leaf is returned unchanged (it is already its own root).
// A branching tree produces one inverted tree per leaf path.
function invertEffectTree(tree: EffectTreeResponse): EffectTreeResponse[] {
    return collectPaths(tree).map(path => pathToInvertedTree([...path].reverse()));
}

// Class to present the locations of the effects as a TreeView in VSCode's sidebar
export class LocationsProvider implements vscode.TreeDataProvider<vscode.TreeItem> {
    private _onDidChangeTreeData: vscode.EventEmitter<vscode.TreeItem | undefined> =
        new vscode.EventEmitter<vscode.TreeItem | undefined>();
    readonly onDidChangeTreeData: vscode.Event<vscode.TreeItem | undefined> =
        this._onDidChangeTreeData.event;

    private currentFilters: string[] = ["[All]"];
    private audited: Set<EffectResponseData> = new Set();
    private callStack?: Map<string, EffectResponseData[]>;
    private groupedEffects:  { [file: string]: EffectResponseData[] } = {};
    private filteredEffects: { [file: string]: EffectResponseData[] } = {};
    // Audit-mode tree structures (keyed by file path, then root function name)
    private auditTreesByFunc: AuditFileMap = {};
    private filteredAuditTreesByFunc: AuditFileMap = {};
    private isAuditMode: boolean = false;
    // Dependency ranking based on concentration of propagated effects
    private depRankings: Map<string, DepRankInfo> = new Map();
    // Maps the first path segment of a relative file path to its crate_name
    private dirToCrateName: Map<string, string> = new Map();

    setLocations(
        effects: EffectResponseData[],
        callStack?: Map<string, EffectResponseData[]>
    ) {
        this.isAuditMode = false;
        if (callStack !== undefined) {
            this.callStack = callStack;
        }
        this.groupByFile(effects);
        this.sortGroupedEffects();
        this.filterEffectsByType(this.currentFilters);
        this.refresh();
    }

    setAuditLocations(
        entries: [EffectResponseData, EffectTreeResponse][],
        callStackMap: Map<string, EffectResponseData[]>
    ) {
        this.isAuditMode = true;
        this.callStack = callStackMap;

        for (const [, tree] of entries) {
            // Flatten effect trees
            // (needed for counts, safety annotations, getGroupedEffects())
            this.addToGroupedEffects(tree.info);
            this.collectFlatChildren(tree);

            // Store inverted trees so the outermost caller is the root
            for (const inverted of invertEffectTree(tree)) {
                const file = inverted.info.location.uri.fsPath;
                const funcName = inverted.info.caller;
                if (!this.auditTreesByFunc[file]) this.auditTreesByFunc[file] = {};
                if (!this.auditTreesByFunc[file][funcName]) this.auditTreesByFunc[file][funcName] = [];
                this.auditTreesByFunc[file][funcName].push(inverted);
            }
        }

        this.sortGroupedEffects();
        this.filterEffectsByType(this.currentFilters);
        this.refresh();
    }

    private addToGroupedEffects(effect: EffectResponseData) {
        const file = effect.location.uri.fsPath;
        if (!this.groupedEffects[file]) this.groupedEffects[file] = [];
        if (!this.groupedEffects[file].some(e =>
            e.location.range.isEqual(effect.location.range) && e.callee === effect.callee
        )) {
            this.groupedEffects[file].push(effect);
        }
    }

    private collectFlatChildren(tree: EffectTreeResponse) {
        for (const child of tree.children) {
            this.addToGroupedEffects(child.info);
            this.collectFlatChildren(child);
        }
    }

    updateEffectCallStack(baseEffect: EffectResponseData, callers: EffectResponseData[]) {
        const baseEffectStr = JSON.stringify(baseEffect);
        this.callStack = this.callStack ?? new Map<string, EffectResponseData[]>();

        if (this.callStack.has(baseEffectStr)) {
            const currentStack = this.callStack.get(baseEffectStr);
            if (currentStack) {
                this.callStack.set(baseEffectStr, [...currentStack, ...callers]);
            }
        } else {
            this.callStack.set(baseEffectStr, [baseEffect, ...callers]);
        }

        this.refresh();
    }

    setDepRankings(rankings: DepRankInfo[]) {
        this.depRankings.clear();
        for (const r of rankings) {
            this.depRankings.set(r.crate_name, r);
        }
    }

    addAuditedEffects(effects: EffectResponseData[]) {
        effects.forEach(e => this.audited.add(e));
        this.refresh();
    }

    unmarkAuditedEffect(effect: EffectResponseData) {
        this.audited.delete(effect);
        this.refresh();
    }

    getTreeItem(element: vscode.TreeItem): vscode.TreeItem {
        return element;
    }

    getChildren(element?: vscode.TreeItem): Thenable<vscode.TreeItem[]> {
        if (!element) {
            const crates = this.getCrateItems();
            if (crates.length <= 1) {
                crates.forEach(c => { if (c instanceof DirectoryItem) c.updateDescription(); });
                return Promise.resolve(crates);
            }

            const root = new DirectoryItem(vscode.Uri.file("root"), "Root Crate");
            const deps = new DirectoryItem(vscode.Uri.file("dependencies"), "Dependencies");
            const wsName = vscode.workspace.workspaceFolders?.map((folder) => folder.name)[0];

            const depCrates: vscode.TreeItem[] = [];
            crates.forEach(crate => crate.label === wsName ? root.addChild(crate) : depCrates.push(crate));

            // Sort deps by concentration of effects
            depCrates.sort((a, b) => {
                const aLabel = typeof a.label === 'string' ? a.label : a.label?.label ?? '';
                const bLabel = typeof b.label === 'string' ? b.label : b.label?.label ?? '';
                const aRank = this.depRankings.get(this.dirToCrateName.get(aLabel) ?? '');
                const bRank = this.depRankings.get(this.dirToCrateName.get(bLabel) ?? '');
                
                if (!aRank && !bRank) return aLabel.localeCompare(bLabel);
                if (!aRank) return 1;
                if (!bRank) return -1;
                
                const aTotal = aRank.base_effects + aRank.propagated_effects;
                const bTotal = bRank.base_effects + bRank.propagated_effects;
                
                if (bTotal !== aTotal) return bTotal - aTotal;
                return aRank.topo_index - bRank.topo_index;
            });

            depCrates.forEach(crate => {
                const label = typeof crate.label === 'string' ? crate.label : crate.label?.label ?? '';
                const rank = this.depRankings.get(this.dirToCrateName.get(label) ?? '');
                if (rank && crate instanceof DirectoryItem) {
                    crate.setRankInfo(`↑${rank.base_effects + rank.propagated_effects}`);
                }
                deps.addChild(crate);
            });

            crates.forEach(c => { if (c instanceof DirectoryItem) c.updateDescription(); });

            root.updateDecorations(false);
            deps.updateDecorations(true);

            return Promise.resolve([root, deps]);
        } else if (element instanceof DirectoryItem) {
            return Promise.resolve(element.getChildren());
        } else if (element instanceof FileItem) {
            return Promise.resolve(element.getChildren());
        } else if (element instanceof FunctionItem) {
            return Promise.resolve(element.getChildren());
        }
        return Promise.resolve([]);
    }

    // Group effects by their containing file
    private groupByFile(effects: EffectResponseData[]) {
        for (const effect of effects) {
            const uri = effect.location.uri;
            const file = uri.fsPath;

            if (!this.groupedEffects[file]) {
                this.groupedEffects[file] = [];
            }

            if (!this.groupedEffects[file].some(e => e.location.range.isEqual(effect.location.range)
                && e.callee == effect.callee)) {
                this.groupedEffects[file].push(effect);
            }
        }
    }

    private getCrateItems(): vscode.TreeItem[] {
        const crates: vscode.TreeItem[] = [];

        const files = this.isAuditMode
            ? Object.keys(this.filteredAuditTreesByFunc)
            : Object.keys(this.filteredEffects);

        for (const file of files) {
            const effects = this.filteredEffects[file] ?? [];
            if (!this.isAuditMode && effects.length === 0) continue;

            const firstEffect = effects[0] ?? this.getFirstFromAuditTrees(file);
            if (!firstEffect) continue;

            const crateName = firstEffect.crate_name;
            const relativePath = this.getRelativeFilePath(file, crateName);
            const dirLabel = relativePath.split(path.sep)[0];
            if (!this.dirToCrateName.has(dirLabel)) {
                this.dirToCrateName.set(dirLabel, crateName);
            }
            const auditFuncMap = this.isAuditMode ? this.filteredAuditTreesByFunc[file] : undefined;
            this.buildDirectories(relativePath, effects, crates, auditFuncMap);
        }

        // Sort alphabetically by crate name
        return crates.sort((a, b) => {
            const aLabel = typeof a.label === 'string' ? a.label : a.label?.label ?? '';
            const bLabel = typeof b.label === 'string' ? b.label : b.label?.label ?? '';
            return aLabel.localeCompare(bLabel);
        });
    }

    private getFirstFromAuditTrees(file: string): EffectResponseData | undefined {
        const funcMap = this.filteredAuditTreesByFunc[file];
        if (!funcMap) return undefined;
        for (const trees of Object.values(funcMap)) {
            if (trees.length > 0) return trees[0].info;
        }
        return undefined;
    }

    private getRelativeFilePath(filePath: string, crateName: string): string {
        const segments = filePath.split(path.sep);
        const name = crateName.replace(/_/g, '-');
        const idx = segments.findIndex((segment) => {
            return segment.replace(/_/g, '-').startsWith(name);
        });

        return idx !== -1 ? path.join(...segments.slice(idx)) : filePath;
    }

    private buildDirectories(
        filePath: string,
        effects: EffectResponseData[],
        parentItems: vscode.TreeItem[],
        auditFuncMap?: AuditFuncMap
    ) {
        const segments = filePath.split(path.sep);
        segments.forEach((segment, index) => {
            if (index === segments.length - 1) {
                const fileItem = new FileItem(
                    vscode.Uri.file(filePath),
                    effects,
                    this.audited,
                    this.callStack,
                    auditFuncMap
                );
                parentItems.push(fileItem);
            } else {
                let dirItem = parentItems.find(
                    (item) => item instanceof DirectoryItem && item.label === segment
                ) as DirectoryItem;

                if (!dirItem) {
                    const dirPath = path.join(...segments.slice(0, index+1));
                    dirItem = new DirectoryItem(vscode.Uri.file(dirPath), segment);
                    parentItems.push(dirItem);
                }

                parentItems = dirItem.getChildren();
            }
        });
    }

    getGroupedEffects(): { [file: string]: EffectResponseData[] } {
        return this.groupedEffects;
    }

    register(context: vscode.ExtensionContext) {
        vscode.window.registerFileDecorationProvider(DecorationProvider);
        const tree = vscode.window.createTreeView('effectsView', { treeDataProvider: this, showCollapseAll: true });
        tree.onDidChangeCheckboxState(async (event) => {
            const item = event.items[0];
            const effect = (item[0] as LocationItem).data;

            if (item[1] === vscode.TreeItemCheckboxState.Unchecked) {
                const result = await vscode.window.showWarningMessage(
                    'Are you sure you want to reset this annotation?',
                    { modal: true },
                    'Yes',
                    'No'
                );

                if ( result === 'Yes' ) {
                    vscode.commands.executeCommand('cargo-scan.set_annotation', effect, 'Skipped');
                }
                else {
                    item[0].checkboxState = vscode.TreeItemCheckboxState.Checked;
                    this.refresh();
                }
            }
        });

        context.subscriptions.push(tree);
    }

    clear() {
        this.audited.clear();
        this.groupedEffects = {};
        this.filteredEffects = {};
        this.auditTreesByFunc = {};
        this.filteredAuditTreesByFunc = {};
        this.callStack = undefined;
        this.currentFilters = ["[All]"];
        this.isAuditMode = false;
        this.depRankings.clear();
        this.dirToCrateName.clear();
        this.refresh();
    }

    refresh() {
        this._onDidChangeTreeData.fire(undefined);
    }

    restore() {
        this.filteredEffects = {};
        this.filteredAuditTreesByFunc = {};
        this.currentFilters = ["[All]"];
        // Re-enable audit mode if we have audit trees stored
        this.isAuditMode = Object.keys(this.auditTreesByFunc).length > 0;
        this.filterEffectsByType(this.currentFilters);
        this.refresh();
    }

    private sortGroupedEffects() {
        for (const file in this.groupedEffects) {
            this.groupedEffects[file].sort((a, b) =>
                a.location.range.start.compareTo(b.location.range.start));
        }
    }

    filterEffectsByType(filters: string[]) {
        if (!filters) return;

        this.currentFilters = [...filters];

        const matchesFilter = (e: EffectResponseData) => {
            const ty = e.effect_type.startsWith('[') ? e.effect_type : "[Sink Call]";
            return filters.includes(ty);
        };

        if (filters.includes("[All]")) {
            this.filteredEffects = { ...this.groupedEffects };
            if (this.isAuditMode) {
                this.filteredAuditTreesByFunc = { ...this.auditTreesByFunc };
            }
            this.refresh();
            return;
        }

        for (const [file, effects] of Object.entries(this.groupedEffects)) {
            this.filteredEffects[file] = effects.filter(matchesFilter);
        }

        if (this.isAuditMode) {
            this.filteredAuditTreesByFunc = {};
            for (const [file, funcMap] of Object.entries(this.auditTreesByFunc)) {
                for (const [funcName, trees] of Object.entries(funcMap)) {
                    // Filter by the root effect type (all nodes in a tree share the same type)
                    const filtered = trees.filter(t => matchesFilter(t.info));
                    if (filtered.length > 0) {
                        if (!this.filteredAuditTreesByFunc[file]) this.filteredAuditTreesByFunc[file] = {};
                        this.filteredAuditTreesByFunc[file][funcName] = filtered;
                    }
                }
            }
        }

        this.refresh();
    }

    filterByCallers(item: LocationItem) {
        if (item.contextValue == 'isCC') {
            const callers = this.callStack?.get(JSON.stringify(item.data));
            if (callers && callers.length > 1) {
                this.isAuditMode = false;
                this.filteredEffects = {};

                for (const caller of callers) {
                    const file = caller.location.uri.fsPath;
                    if (!this.filteredEffects[file]) {
                        this.filteredEffects[file] = [];
                    }
                    this.filteredEffects[file].push(caller);
                }
            }
            this.refresh();
        }
    }
}

class DirectoryItem extends vscode.TreeItem {
    private children: vscode.TreeItem[] = [];
    private total: number;
    private unaudited: number;
    private rankInfo: string = '';

    constructor(
        public readonly resourceUri: vscode.Uri,
        public readonly label: string
    ) {
        super(label, vscode.TreeItemCollapsibleState.Collapsed);
        this.total = 0;
        this.unaudited = 0;
    }

    getChildren(): vscode.TreeItem[] {
        return this.children;
    }

    totalEffects(): number {
        return this.total;
    }

    totalUnaudited(): number {
        return this.unaudited;
    }

    updateDescription() {
        for (const child of this.children) {
            if (child instanceof DirectoryItem) {
                child.updateDescription();
                this.total += child.totalEffects();
                this.unaudited += child.totalUnaudited();
            }
            else if (child instanceof FileItem) {
                this.total += child.totalEffects();
                this.unaudited += child.totalUnaudited();
            }
        }

        const counts = this.total > 0 ? `[ ${this.total - this.unaudited} / ${this.total} ]` : '';
        const parts = [counts, this.rankInfo].filter(s => s.length > 0);
        this.description = parts.length > 0 ? parts.join('  ') : undefined;

        DecorationProvider.updateDecorations(this.resourceUri, this.unaudited);
    }

    updateDecorations(deps: boolean) {
        const symbol = deps ? 'library' : 'package';
        this.iconPath = new vscode.ThemeIcon(symbol);
        DecorationProvider.decorateChainRoots(this.resourceUri);
    }

    addChild(item: vscode.TreeItem) {
        this.children.push(item);
    }

    setRankInfo(info: string) {
        this.rankInfo = info;
    }
}

class FileItem extends vscode.TreeItem {
    private total: number = 0;
    private unaudited: number = 0;
    constructor(
        public readonly resourceUri: vscode.Uri,
        public readonly effects: EffectResponseData[],
        private readonly audited: Set<EffectResponseData>,
        private readonly callStack?: Map<string, EffectResponseData[]>,
        private readonly auditFuncMap?: AuditFuncMap
    ) {
        const label = `${path.basename(resourceUri.fsPath)}`;
        super(label, vscode.TreeItemCollapsibleState.Collapsed);
        this.iconPath = vscode.ThemeIcon.File;
        this.updateDecorations();
    }

    private updateDecorations() {
        this.total = this.effects.length;
        this.unaudited = this.effects.reduce((count, item) => {
            return !this.audited.has(item) ? count += 1 : count;
        }, 0);

        this.description = `[ ${this.total - this.unaudited} / ${this.total} ]`;
        DecorationProvider.updateDecorations(this.resourceUri, this.unaudited);
    }

    totalEffects(): number {
        return this.total;
    }

    totalUnaudited(): number {
        return this.unaudited;
    }

    getChildren(): vscode.TreeItem[] {
        if (this.auditFuncMap) {
            // Audit mode: make tree view changes only for audit binaries for now
            return Object.entries(this.auditFuncMap)
                .sort(([a], [b]) => a.localeCompare(b))
                .map(([funcName, trees]) =>
                    new FunctionItem(funcName, trees, this.audited, this.callStack)
                );
        }
        // Scan / get_callers mode: flattened list of effects as defore
        // TODO: make this similar to auditing
        return this.effects.map((effect) => {
            const state = this.audited.has(effect)
                ? vscode.TreeItemCheckboxState.Checked : undefined;
            const isCCBaseEffect = (this.callStack?.get(JSON.stringify(effect))?.length ?? 0) > 1;
            return new LocationItem(effect, state, isCCBaseEffect);
        });
    }
}

// Represents one function in the (inverted) effect call chain.
class FunctionItem extends vscode.TreeItem {
    constructor(
        public readonly funcName: string,
        private readonly trees: EffectTreeResponse[],
        private readonly audited: Set<EffectResponseData>,
        private readonly callStack?: Map<string, EffectResponseData[]>,
    ) {
        const shortName = funcName.split('::').pop() ?? funcName;
        super(shortName, vscode.TreeItemCollapsibleState.Expanded);
        this.tooltip = funcName;
        this.description = `(${trees.length})`;
        this.iconPath = new vscode.ThemeIcon('symbol-function');
    }

    getChildren(): vscode.TreeItem[] {
        const children: vscode.TreeItem[] = [];
        // Track info object identity to deduplicate: branching produces multiple inverted
        // trees that share the same intermediate node (same EffectResponseData reference)
        const seen = new Set<EffectResponseData>();

        // Emit a LocationItem only for the actual base effects,
        // not the propagated callers
        for (const tree of this.trees) {
            if (tree.children.length === 0 && !seen.has(tree.info)) {
                seen.add(tree.info);
                const state = this.audited.has(tree.info)
                    ? vscode.TreeItemCheckboxState.Checked : undefined;
                const isCCBaseEffect = (this.callStack?.get(JSON.stringify(tree.info))?.length ?? 0) > 1;
                children.push(new LocationItem(tree.info, state, isCCBaseEffect));
            }
        }

        // Descend into callees (going deeper in the inverted tree toward the base effect),
        // deduplicating by info identity for the same effect tree branches
        const childrenByFunc = new Map<string, EffectTreeResponse[]>();
        for (const tree of this.trees) {
            for (const child of tree.children) {
                if (seen.has(child.info)) continue;
                seen.add(child.info);
                const fn = child.info.caller;
                if (!childrenByFunc.has(fn)) childrenByFunc.set(fn, []);
                childrenByFunc.get(fn)!.push(child);
            }
        }

        for (const [fn, subTrees] of [...childrenByFunc.entries()].sort(([a], [b]) => a.localeCompare(b))) {
            children.push(new FunctionItem(fn, subTrees, this.audited, this.callStack));
        }

        return children;
    }
}

export class LocationItem extends vscode.TreeItem {
    constructor(
        public readonly data: EffectResponseData,
        public readonly state: vscode.TreeItemCheckboxState | undefined,
        private readonly isCCBaseEffect: boolean
    ) {
        let start = data.location.range.start;
        super(
            `${data.effect_type}`,
            vscode.TreeItemCollapsibleState.None
        );

        this.checkboxState = state;
        this.tooltip = data.callee;
        this.command = {
            command: 'vscode.open',
            title: 'Open Location',
            arguments: [data.location.uri, { selection: data.location.range }]
        };
        this.description = `${start.line + 1}:${start.character + 1}`;
        this.contextValue = this.isCCBaseEffect ? 'isCC' : undefined;
    }
}
