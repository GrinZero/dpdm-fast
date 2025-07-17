use crate::parser::types::{ExportSymbol, ImportSymbol};

use super::consts::DependencyKind;
use super::types::Dependency;
use std::path::PathBuf;
use swc_core::ecma::ast::{Callee, ImportSpecifier, ModuleExportName};
use swc_core::ecma::utils::swc_ecma_ast;
use swc_core::ecma::visit::{Visit, VisitWith};

pub struct DependencyCollector {
    pub path: PathBuf,
    pub dependencies: Vec<Dependency>,
    pub id: String,
    pub skip_dynamic_imports: bool,
    pub collect_symbol: bool,
    pub imports: Vec<ImportSymbol>,
    pub exports: Vec<ExportSymbol>,
}

impl Visit for DependencyCollector {
    fn visit_import_decl(&mut self, import: &swc_ecma_ast::ImportDecl) {
        // 处理静态导入
        let request = import.src.value.to_string();
        let dependency = Dependency {
            issuer: self.path.to_string_lossy().to_string(),
            request: request.clone(),
            kind: DependencyKind::StaticImport,
            id: Some(self.id.clone()),
        };
        self.dependencies.push(dependency);

        if !self.collect_symbol {
            return;
        }

        for specifier in &import.specifiers {
            match specifier {
                ImportSpecifier::Named(named) => {
                    let local = named.local.sym.to_string();
                    let imported = match &named.imported {
                        Some(ModuleExportName::Ident(ident)) => ident.sym.to_string(),
                        Some(ModuleExportName::Str(s)) => s.value.to_string(),
                        None => local.clone(),
                    };
                    self.imports.push(ImportSymbol {
                        local,
                        imported,
                        source: request.clone(),
                    });
                }
                ImportSpecifier::Default(default) => {
                    self.imports.push(ImportSymbol {
                        local: default.local.sym.to_string(),
                        imported: "default".to_string(),
                        source: request.clone(),
                    });
                }
                ImportSpecifier::Namespace(ns) => {
                    self.imports.push(ImportSymbol {
                        local: ns.local.sym.to_string(),
                        imported: "*".to_string(),
                        source: request.clone(),
                    });
                }
            }
        }
    }

    fn visit_call_expr(&mut self, expr: &swc_ecma_ast::CallExpr) {
        if self.skip_dynamic_imports {
            return;
        }
        if let Callee::Import(_) = &expr.callee {
            if let Some(arg) = expr.args.get(0) {
                if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(ref s)) = *arg.expr {
                    let request = s.value.to_string();
                    let dependency = Dependency {
                        issuer: self.path.to_string_lossy().to_string(),
                        request,
                        kind: DependencyKind::DynamicImport,
                        id: Some(self.id.clone()),
                    };
                    self.dependencies.push(dependency);
                }
            }
        }

        if let swc_ecma_ast::Callee::Expr(ref callee_expr) = expr.callee {
            // 处理 CommonJS 导入
            if let swc_ecma_ast::Expr::Ident(ref ident) = &**callee_expr {
                if ident.sym == *"require" {
                    if let Some(arg) = expr.args.get(0) {
                        if let swc_ecma_ast::Expr::Lit(swc_ecma_ast::Lit::Str(ref s)) = *arg.expr {
                            let request = s.value.to_string();
                            let dependency = Dependency {
                                issuer: self.path.to_string_lossy().to_string(),
                                request,
                                kind: DependencyKind::CommonJS,
                                id: Some(self.id.clone()),
                            };
                            self.dependencies.push(dependency);
                        }
                    }
                }
            }
        }
        expr.visit_children_with(self);
    }

    fn visit_export_named_specifier(&mut self, export: &swc_ecma_ast::ExportNamedSpecifier) {
        // 处理静态导出
        if let Some(src) = &export.exported {
            let request = match src {
                swc_ecma_ast::ModuleExportName::Ident(ident) => ident.sym.to_string(),
                swc_ecma_ast::ModuleExportName::Str(s) => s.value.to_string(),
            };

            let dependency = Dependency {
                issuer: self.path.to_string_lossy().to_string(),
                request,
                kind: DependencyKind::StaticExport,
                id: Some(self.id.clone()),
            };
            self.dependencies.push(dependency);
        }
    }

    fn visit_export_all(&mut self, node: &swc_ecma_ast::ExportAll) {
        let request = node.src.value.to_string();
        let dependency = Dependency {
            issuer: self.path.to_string_lossy().to_string(),
            request,
            kind: DependencyKind::StaticExport,
            id: Some(self.id.clone()),
        };
        self.dependencies.push(dependency);
    }

    fn visit_named_export(&mut self, export: &swc_ecma_ast::NamedExport) {
        if !self.collect_symbol {
            return;
        }

        // export { foo as bar } from './mod'
        let request = export.src.as_ref().map(|src| src.value.to_string());

        for specifier in &export.specifiers {
            match specifier {
                swc_ecma_ast::ExportSpecifier::Named(named) => {
                    let local = match &named.orig {
                        swc_ecma_ast::ModuleExportName::Ident(ident) => ident.sym.to_string(),
                        swc_ecma_ast::ModuleExportName::Str(s) => s.value.to_string(),
                    };
                    let exported = match &named.exported {
                        Some(ModuleExportName::Ident(ident)) => ident.sym.to_string(),
                        Some(ModuleExportName::Str(s)) => s.value.to_string(),
                        None => local.clone(),
                    };

                    self.exports.push(ExportSymbol {
                        local: local.clone(),
                        exported,
                        reexport_source: request.clone(),
                    });

                    if let Some(req) = &request {
                        let dependency = Dependency {
                            issuer: self.path.to_string_lossy().to_string(),
                            request: req.clone(),
                            kind: DependencyKind::StaticExport,
                            id: Some(self.id.clone()),
                        };
                        self.dependencies.push(dependency);
                    }
                }
                swc_ecma_ast::ExportSpecifier::Default(_) => {
                    // 不太常见，忽略
                }
                swc_ecma_ast::ExportSpecifier::Namespace(ns) => {
                    // export * as ns from './mod'
                    let exported = match &ns.name {
                        swc_ecma_ast::ModuleExportName::Ident(ident) => ident.sym.to_string(),
                        swc_ecma_ast::ModuleExportName::Str(s) => s.value.to_string(),
                    };
                    self.exports.push(ExportSymbol {
                        local: "*".to_string(),
                        exported,
                        reexport_source: request.clone(),
                    });

                    if let Some(req) = &request {
                        let dependency = Dependency {
                            issuer: self.path.to_string_lossy().to_string(),
                            request: req.clone(),
                            kind: DependencyKind::StaticExport,
                            id: Some(self.id.clone()),
                        };
                        self.dependencies.push(dependency);
                    }
                }
            }
        }
    }

    fn visit_export_decl(&mut self, export: &swc_ecma_ast::ExportDecl) {
        if !self.collect_symbol {
            return;
        }
        // export const foo = ...
        // export function bar() {}
        // export class Baz {}
        let local = match &export.decl {
            swc_ecma_ast::Decl::Class(class) => class.ident.sym.to_string(),
            swc_ecma_ast::Decl::Fn(func) => func.ident.sym.to_string(),
            swc_ecma_ast::Decl::Var(var) => {
                // 只取第一个绑定名
                let decl = &var.decls[0];
                if let Some(ident) = decl.name.as_ident() {
                    ident.sym.to_string()
                } else {
                    "<unknown>".to_string()
                }
            }
            _ => "<unknown>".to_string(),
        };

        self.exports.push(ExportSymbol {
            local: local.clone(),
            exported: local,
            reexport_source: None,
        });
    }

    fn visit_export_default_decl(&mut self, export: &swc_ecma_ast::ExportDefaultDecl) {
        if !self.collect_symbol {
            return;
        }
        let local = match &export.decl {
            swc_ecma_ast::DefaultDecl::Class(class) => class
                .ident
                .as_ref()
                .map(|id| id.sym.to_string())
                .unwrap_or("default".to_string()),
            swc_ecma_ast::DefaultDecl::Fn(func) => func
                .ident
                .as_ref()
                .map(|id| id.sym.to_string())
                .unwrap_or("default".to_string()),
            swc_ecma_ast::DefaultDecl::TsInterfaceDecl(_) => "default".to_string(),
        };

        self.exports.push(ExportSymbol {
            local: local.clone(),
            exported: "default".to_string(),
            reexport_source: None,
        });
    }

    fn visit_export_default_expr(&mut self, _export: &swc_ecma_ast::ExportDefaultExpr) {
        if !self.collect_symbol {
            return;
        }
        self.exports.push(ExportSymbol {
            local: "default".to_string(),
            exported: "default".to_string(),
            reexport_source: None,
        });
    }
}
