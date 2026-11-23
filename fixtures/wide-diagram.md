Wide diagram

A `flowchart LR` with nested subgraphs, about twenty nodes, for the diagram zoom view. It is
wider than the page's column, so the page draws it shrunk and a click opens it at a reading size.

```mermaid
flowchart LR
  subgraph home[Home office]
    router[Router]
    switch[Switch]
    desktop[Desktop]
    laptop[Laptop]
    printer[Printer]
  end
  subgraph media[Media shelf]
    nas[NAS]
    tv[Television]
    speakers[Speakers]
  end
  subgraph services[Services]
    dns[DNS]
    proxy[Proxy]
    metrics[Metrics]
    backups[Backups]
  end
  subgraph remote[Remote]
    phone[Phone]
    tablet[Tablet]
    guest[Guest network]
  end
  router --> switch
  switch --> desktop
  switch --> laptop
  switch --> printer
  switch --> nas
  nas --> tv
  nas --> speakers
  router --> dns
  dns --> proxy
  proxy --> metrics
  nas --> backups
  router --> phone
  router --> tablet
  router --> guest
  desktop --> proxy
  laptop --> proxy
  phone --> proxy
  tablet --> proxy
```

After the diagram, a paragraph, so the page has text above and below it.
