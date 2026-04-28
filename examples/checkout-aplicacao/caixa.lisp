;; checkout — canonical demonstration of `:kind Aplicacao`.
;;
;; Composes four caixa Servicos into a typed mesh:
;;   catalog → cart → payment → fulfillment
;; with a public ingress at checkout.quero.cloud → cart.
;;
;; Render the cluster artifacts:
;;   feira app graph                   — print the typed graph for review
;;   feira app deploy --cluster rio --dry-run
;;       — preview programs.yaml entries + Cilium NetworkPolicies +
;;         Gateway/HTTPRoute pair
;;   feira app deploy --cluster rio --apply
;;       — write to the k8s repo + commit + push
;;
;; The Servicos themselves are independent caixa repos (catalog,
;; cart, payment, fulfillment), each :kind Servico with their own
;; caixa.lisp + servicos/<name>.computeunit.yaml. This Aplicacao
;; declares only the typed graph + mesh policies; the substrate's
;; existing primitives (caixa-flux, caixa-helm, wasm-operator)
;; handle the per-Servico runtime.

(defcaixa
  :nome           "checkout"
  :versao         "0.1.0"
  :kind           Aplicacao
  :edicao         "2026"
  :descricao      "Canonical Aplicacao example — checkout flow with four typed Servicos and Cilium-emitted L7 mesh policies."
  :repositorio    "github:pleme-io/caixa/examples/checkout-aplicacao"
  :licenca        "MIT"
  :autores        ("pleme-io")
  :etiquetas      ("example" "aplicacao" "mesh" "ecommerce" "demo")

  :membros        ((:caixa "catalog"     :versao "^0.1")
                   (:caixa "cart"        :versao "^0.1")
                   (:caixa "payment"     :versao "^0.2")
                   (:caixa "fulfillment" :versao "^0.1"))

  :contratos      ((:de "cart"        :para "catalog"
                    :wit "wasi:http/proxy" :endpoint "/products/:id")
                   (:de "cart"        :para "payment"
                    :wit "wasi:http/proxy" :endpoint "/charge")
                   (:de "payment"     :para "fulfillment"
                    :wit "nats:pub-sub"     :subject "rio.events.order.charged"))

  ;; :mtls-required defaults to true at the substrate level (Cilium
  ;; identity-based authorization is on by default once Cilium is in
  ;; the cluster). Authoring an explicit boolean in tatara-lisp needs
  ;; the derive macro to grow boolean support — tracked separately.
  :politicas      (:timeout         "30s"
                   :retries         3)

  :placement      (:estrategia Replicated
                   :clusters   ("rio" "mar")
                   :affinity   "data-locality")

  :entrada        (:host  "checkout.quero.cloud"
                   :para  "cart"
                   :paths ("/api/cart" "/api/products")
                   :port  8080))
