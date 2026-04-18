;; pangea-tatara-akeyless — a tatara-lisp authored infrastructure caixa.
;;
;; Consumed by:
;;   feira tofu render                  ;; proof + HCL emission, no cloud
;;   feira tofu plan                    ;; full tofu init && tofu plan
;;   feira tofu apply                   ;; interactive
;;
;; Every (defteia …) in infra/ is walked through caixa-arch invariants
;; BEFORE HCL is written. Safety violations block emission.

(defcaixa
  :nome        "pangea-tatara-akeyless"
  :versao      "0.1.0"
  :kind        Biblioteca
  :edicao      "2026"
  :descricao   "Example Lisp-authored infrastructure — AWS VPC + Akeyless target."
  :repositorio "github:pleme-io/caixa"
  :licenca     "MIT"
  :autores     ("pleme-io")
  :etiquetas   ("example" "infrastructure" "aws" "akeyless" "pangea-native")
  :deps        ((:nome "caixa-teia" :versao "^0.1"
                 :fonte (:tipo git :repo "github:pleme-io/caixa" :tag "v0.1.0"))
                (:nome "caixa-arch" :versao "^0.1"
                 :fonte (:tipo git :repo "github:pleme-io/caixa" :tag "v0.1.0")))
  :bibliotecas ("lib/pangea-tatara-akeyless.lisp"))
